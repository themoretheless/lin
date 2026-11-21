use crate::error::Error;

use super::client::send_peer_rpc;
use super::cluster::{NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftLogEntry};
use super::protocol::{CacheRequest, CacheResponse};
use super::server::CacheNode;
use super::unix_ms;

impl CacheNode {
    /// Parallel inter-node RPC broadcast: 0 peers = no-op, 1 peer = inline on calling thread,
    /// `K > 1` peers = `K - 1` scoped threads + last peer inline on calling thread.
    pub(crate) fn broadcast_peer_rpc(peers: &[String], rpc: &CacheRequest) {
        match peers {
            [] => {}
            [only] => {
                let _ = send_peer_rpc(only, rpc);
            }
            [rest @ .., last] => {
                std::thread::scope(|s| {
                    for peer_addr in rest {
                        s.spawn(move || {
                            let _ = send_peer_rpc(peer_addr, rpc);
                        });
                    }
                    let _ = send_peer_rpc(last, rpc);
                });
            }
        }
    }

    fn collect_peer_rpcs(peers: &[String], rpc: &CacheRequest) -> Vec<Result<CacheResponse, Error>> {
        match peers {
            [] => Vec::new(),
            [only] => vec![send_peer_rpc(only, rpc)],
            [rest @ .., last] => std::thread::scope(|s| {
                let mut handles = Vec::with_capacity(rest.len());
                for peer_addr in rest {
                    handles.push(s.spawn(move || send_peer_rpc(peer_addr, rpc)));
                }
                let last_res = send_peer_rpc(last, rpc);
                let mut out: Vec<Result<CacheResponse, Error>> = handles
                    .into_iter()
                    .filter_map(|h| h.join().ok())
                    .collect();
                out.push(last_res);
                out
            }),
        }
    }

    /// Held from reading the current value until the resulting command is applied and
    /// sent, so concurrent read-modify-write commands cannot overwrite each other and
    /// peers receive entries in index order.
    pub(crate) fn mutation_order(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_order
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn propose_and_apply_mutation(&self, cmd: RaftCommand) {
        let order = self.mutation_order();
        self.propose(&order, cmd, true);
    }

    pub(crate) fn propose_and_apply_held(
        &self,
        order: &std::sync::MutexGuard<'_, ()>,
        cmd: RaftCommand,
    ) {
        self.propose(order, cmd, true);
    }

    pub(crate) fn propose_raft_only(&self, cmd: RaftCommand) {
        let order = self.mutation_order();
        self.propose(&order, cmd, false);
    }

    fn propose(&self, _order: &std::sync::MutexGuard<'_, ()>, cmd: RaftCommand, apply: bool) {
        let (entry, prev_idx, prev_term, term, peers) = {
            let mut st = self.state.write().unwrap();
            let prev_idx = st.raft.last_log_index();
            let prev_term = st.raft.last_log_term();
            let term = st.raft.current_term;
            let idx = prev_idx + 1;
            if apply {
                st.apply_raft_command(&cmd, self.cfg.max_entries, self.cfg.max_bytes);
            }
            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd,
            };
            let peers: Vec<String> = if self.cfg.raft_replication && st.members.len() > 1 {
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                    .map(|m| m.addr.clone())
                    .collect()
            } else {
                Vec::new()
            };
            let entry_for_rpc = if peers.is_empty() {
                None
            } else {
                Some(entry.clone())
            };
            st.raft.log.push(entry);
            st.raft.commit_index = idx;
            st.raft.last_applied = idx;
            st.raft.maybe_compact();
            (entry_for_rpc, prev_idx, prev_term, term, peers)
        };

        let Some(entry) = entry else {
            return;
        };
        let rpc = CacheRequest::RaftAppendEntries {
            term,
            leader_id: self.cfg.node_id.clone(),
            leader_addr: self.cfg.listen_addr.clone(),
            prev_log_index: prev_idx,
            prev_log_term: prev_term,
            entries: vec![entry],
            leader_commit: prev_idx + 1,
        };
        Self::broadcast_peer_rpc(&peers, &rpc);
    }

    pub(crate) fn handle_gossip_sync(
        &self,
        sender_id: String,
        sender_addr: String,
        incoming_members: Vec<NodeMember>,
        pubsub_events: Vec<PubSubEvent>,
    ) -> CacheResponse {
        let mut st = self.state.write().unwrap();
        let now = unix_ms();

        st.ring.add_node(&sender_id, &sender_addr);
        st.merge_gossip_members(&self.cfg.node_id, incoming_members, now);
        self.sync_has_peers(&st);

        // Process incoming Pub/Sub events
        for ev in pubsub_events {
            if st.pubsub.mark_seen(ev.id) {
                st.publish_local(&ev.channel, &ev.payload, &ev.origin_node);
            }
        }

        // Update our own heartbeat in membership table
        st.sync_local_member(&self.cfg.node_id, true, now);

        let members = st.members.values().cloned().collect();
        CacheResponse::GossipAck {
            node_id: self.cfg.node_id.clone(),
            members,
        }
    }

    pub(crate) fn handle_raft_request_vote(
        &self,
        term: u64,
        candidate_id: String,
        last_log_index: u64,
        last_log_term: u64,
    ) -> CacheResponse {
        let mut st = self.state.write().unwrap();
        if term < st.raft.current_term {
            return CacheResponse::RaftVoteReply {
                term: st.raft.current_term,
                vote_granted: false,
            };
        }
        if term > st.raft.current_term {
            st.raft.current_term = term;
            st.raft.role = NodeRole::Follower;
            st.raft.voted_for = None;
        }
        let my_last_term = st.raft.last_log_term();
        let my_last_idx = st.raft.last_log_index();
        let log_ok = last_log_term > my_last_term
            || (last_log_term == my_last_term && last_log_index >= my_last_idx);
        let can_vote = st
            .raft
            .voted_for
            .as_ref()
            .is_none_or(|v| v == &candidate_id);

        if can_vote && log_ok {
            let now = unix_ms();
            st.raft.voted_for = Some(candidate_id);
            st.raft.last_leader_contact_ms = now;
            st.sync_local_member(&self.cfg.node_id, false, now);
            CacheResponse::RaftVoteReply {
                term: st.raft.current_term,
                vote_granted: true,
            }
        } else {
            CacheResponse::RaftVoteReply {
                term: st.raft.current_term,
                vote_granted: false,
            }
        }
    }

    pub(crate) fn handle_raft_append_entries(
        &self,
        term: u64,
        leader_id: String,
        leader_addr: String,
        _prev_log_index: u64,
        _prev_log_term: u64,
        entries: Vec<RaftLogEntry>,
        leader_commit: u64,
    ) -> CacheResponse {
        let mut st = self.state.write().unwrap();
        if term < st.raft.current_term {
            return CacheResponse::RaftAppendReply {
                term: st.raft.current_term,
                success: false,
                match_index: st.raft.last_log_index(),
            };
        }

        let now = unix_ms();
        st.raft.current_term = term;
        st.raft.role = NodeRole::Follower;
        st.ring.add_node(&leader_id, &leader_addr);
        st.raft.leader_id = Some(leader_id);
        st.raft.leader_addr = Some(leader_addr);
        st.raft.last_leader_contact_ms = now;
        self.sync_has_peers(&st);
        st.sync_local_member(&self.cfg.node_id, false, now);

        for entry in entries {
            if entry.index > st.raft.last_log_index() {
                st.apply_raft_command(&entry.command, self.cfg.max_entries, self.cfg.max_bytes);
                st.raft.last_applied = entry.index;
                st.raft.log.push(entry);
            }
        }
        if leader_commit > st.raft.commit_index {
            st.raft.commit_index = leader_commit.min(st.raft.last_log_index());
        }
        st.raft.maybe_compact();

        CacheResponse::RaftAppendReply {
            term: st.raft.current_term,
            success: true,
            match_index: st.raft.last_log_index(),
        }
    }

    /// Trigger a Raft leader election across known cluster peers.
    pub fn run_leader_election(&self) -> bool {
        let (term, last_idx, last_term, peers, total_nodes) = {
            let mut st = self.state.write().unwrap();
            let now = unix_ms();
            st.raft.current_term += 1;
            st.raft.role = NodeRole::Candidate;
            st.raft.voted_for = Some(self.cfg.node_id.clone());
            let term = st.raft.current_term;
            st.sync_local_member(&self.cfg.node_id, false, now);
            let peers: Vec<String> = st
                .members
                .values()
                .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                .map(|m| m.addr.clone())
                .collect();
            let total = st.members.values().filter(|m| m.alive).count().max(1);
            (
                term,
                st.raft.last_log_index(),
                st.raft.last_log_term(),
                peers,
                total,
            )
        };

        let mut votes = 1usize; // Vote for self
        let quorum = (total_nodes / 2) + 1;
        let req = CacheRequest::RaftRequestVote {
            term,
            candidate_id: self.cfg.node_id.clone(),
            last_log_index: last_idx,
            last_log_term: last_term,
        };

        for res in Self::collect_peer_rpcs(&peers, &req) {
            if let Ok(CacheResponse::RaftVoteReply {
                term: reply_term,
                vote_granted,
            }) = res
            {
                if reply_term > term {
                    let mut st = self.state.write().unwrap();
                    let now = unix_ms();
                    st.raft.current_term = reply_term;
                    st.raft.role = NodeRole::Follower;
                    st.raft.voted_for = None;
                    st.sync_local_member(&self.cfg.node_id, false, now);
                    return false;
                }
                if vote_granted {
                    votes += 1;
                }
            }
        }

        let won = votes >= quorum;
        if won {
            let mut st = self.state.write().unwrap();
            if st.raft.current_term == term {
                let now = unix_ms();
                st.raft.role = NodeRole::Leader;
                st.raft.leader_id = Some(self.cfg.node_id.clone());
                st.raft.leader_addr = Some(self.cfg.listen_addr.clone());
                st.sync_local_member(&self.cfg.node_id, false, now);
            }
            drop(st);
            // Send immediate heartbeat AppendEntries in parallel to establish leadership
            let hb = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: last_idx,
                prev_log_term: last_term,
                entries: Vec::new(),
                leader_commit: last_idx,
            };
            Self::broadcast_peer_rpc(&peers, &hb);
        }
        won
    }

    /// Perform one Gossip round with a specific peer address.
    pub fn gossip_once_with(&self, peer_addr: &str) -> Result<(), Error> {
        let req = {
            let mut st = self.state.write().unwrap();
            let now = unix_ms();
            st.sync_local_member(&self.cfg.node_id, true, now);
            CacheRequest::GossipSync {
                sender_id: self.cfg.node_id.clone(),
                sender_addr: self.cfg.listen_addr.clone(),
                members: st.members.values().cloned().collect(),
                pubsub_events: st.pubsub.recent_outbox.iter().cloned().collect(),
            }
        };

        let resp = send_peer_rpc(peer_addr, &req)?;
        if let CacheResponse::GossipAck {
            node_id: _,
            members,
        } = resp
        {
            let mut st = self.state.write().unwrap();
            let now = unix_ms();
            st.merge_gossip_members(&self.cfg.node_id, members, now);
            self.sync_has_peers(&st);
        }
        Ok(())
    }
}
