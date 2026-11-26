use std::sync::atomic::Ordering;

use crate::error::Error;

use super::client::send_peer_rpc;
use super::cluster::{NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftLogEntry, RaftState};
use super::protocol::{CacheRequest, CacheResponse};
use super::server::CacheNode;
use super::unix_ms;

impl CacheNode {
    /// Synchronizes `RaftState` with lock-free single-node `raft_log_idx` progress so
    /// `last_log_index()`, `commit_index`, and `last_applied` stay monotonic across transitions.
    #[inline]
    pub(crate) fn sync_raft_atomic_progress(&self, raft: &mut RaftState) {
        let atomic_idx = self.store.raft_log_idx.load(Ordering::Acquire);
        let raft_idx = raft.last_log_index();
        if atomic_idx > raft_idx {
            if raft.log.is_empty() {
                raft.base_index = atomic_idx;
                raft.base_term = raft.current_term;
            } else if let Some(last) = raft.log.last() {
                let _ = last;
                raft.log.clear();
                raft.base_index = atomic_idx;
                raft.base_term = raft.current_term;
            }
            raft.commit_index = atomic_idx;
            raft.last_applied = atomic_idx;
        } else if raft_idx > atomic_idx {
            self.store.raft_log_idx.store(raft_idx, Ordering::Release);
        }
    }

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

    /// Held across multi-node replicated mutations so peers receive entries in strict index order.
    pub(crate) fn mutation_order(&self) -> std::sync::MutexGuard<'_, ()> {
        self.mutation_order
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub(crate) fn propose_and_apply_mutation(&self, cmd: RaftCommand) {
        let order = self.mutation_order();
        self.propose(&order, cmd, true);
    }

    pub(crate) fn propose_raft_only(&self, cmd: RaftCommand) {
        let order = self.mutation_order();
        self.propose(&order, cmd, false);
    }

    pub(crate) fn propose_raft_only_held(
        &self,
        order: &std::sync::MutexGuard<'_, ()>,
        cmd: RaftCommand,
    ) {
        self.propose(order, cmd, false);
    }

    fn propose(&self, _order: &std::sync::MutexGuard<'_, ()>, cmd: RaftCommand, apply: bool) {
        let (entry_for_rpc, prev_idx, prev_term, term, idx, peers) = {
            let mut raft = self.raft.lock().unwrap();
            self.sync_raft_atomic_progress(&mut raft);
            let prev_idx = raft.last_log_index();
            let prev_term = raft.last_log_term();
            let term = raft.current_term;
            let idx = prev_idx + 1;
            self.store.raft_log_idx.store(idx, Ordering::Release);

            let peers: Vec<String> = if self.cfg.raft_replication {
                let cl = self.cluster.read().unwrap();
                if cl.members.len() > 1 {
                    cl.members
                        .values()
                        .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                        .map(|m| m.addr.clone())
                        .collect()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd.clone(),
            };
            let entry_for_rpc = if peers.is_empty() {
                None
            } else {
                Some(entry.clone())
            };
            raft.log.push(entry);
            raft.commit_index = idx;
            raft.last_applied = idx;
            raft.maybe_compact();
            (entry_for_rpc, prev_idx, prev_term, term, idx, peers)
        };

        if apply {
            self.store.apply_raft_command(
                &self.state,
                &cmd,
                idx,
                self.cfg.max_entries,
                self.cfg.max_bytes,
            );
        }

        let Some(entry) = entry_for_rpc else {
            return;
        };
        let rpc = CacheRequest::RaftAppendEntries {
            term,
            leader_id: self.cfg.node_id.clone(),
            leader_addr: self.cfg.listen_addr.clone(),
            prev_log_index: prev_idx,
            prev_log_term: prev_term,
            entries: vec![entry],
            leader_commit: idx,
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
        let now = unix_ms();
        let (role, term) = {
            let raft = self.raft.lock().unwrap();
            (raft.role.clone(), raft.current_term)
        };

        let members = {
            let mut cl = self.cluster.write().unwrap();
            cl.ring.add_node(&sender_id, &sender_addr);
            cl.merge_gossip_members(&self.cfg.node_id, incoming_members, now);
            self.sync_has_peers(&cl);
            cl.sync_local_member(&self.cfg.node_id, true, role, term, now);
            cl.members.values().cloned().collect()
        };

        if !pubsub_events.is_empty() {
            let mut ps = self.pubsub.lock().unwrap();
            for ev in pubsub_events {
                if ps.mark_seen(ev.id) {
                    ps.publish_local(&ev.channel, &ev.payload, &ev.origin_node);
                }
            }
        }

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
        let mut raft = self.raft.lock().unwrap();
        self.sync_raft_atomic_progress(&mut raft);
        if term < raft.current_term {
            return CacheResponse::RaftVoteReply {
                term: raft.current_term,
                vote_granted: false,
            };
        }
        if term > raft.current_term {
            raft.current_term = term;
            raft.role = NodeRole::Follower;
            raft.voted_for = None;
        }
        let my_last_term = raft.last_log_term();
        let my_last_idx = raft.last_log_index();
        let log_ok = last_log_term > my_last_term
            || (last_log_term == my_last_term && last_log_index >= my_last_idx);
        let can_vote = raft
            .voted_for
            .as_ref()
            .is_none_or(|v| v == &candidate_id);

        if can_vote && log_ok {
            let now = unix_ms();
            raft.voted_for = Some(candidate_id);
            raft.last_leader_contact_ms = now;
            let cur_term = raft.current_term;
            let role = raft.role.clone();
            drop(raft);
            let mut cl = self.cluster.write().unwrap();
            cl.sync_local_member(&self.cfg.node_id, false, role, cur_term, now);
            CacheResponse::RaftVoteReply {
                term: cur_term,
                vote_granted: true,
            }
        } else {
            CacheResponse::RaftVoteReply {
                term: raft.current_term,
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
        let _order = self.mutation_order();
        let (reply_term, match_idx, to_apply) = {
            let mut raft = self.raft.lock().unwrap();
            self.sync_raft_atomic_progress(&mut raft);
            if term < raft.current_term {
                return CacheResponse::RaftAppendReply {
                    term: raft.current_term,
                    success: false,
                    match_index: raft.last_log_index(),
                };
            }

            let now = unix_ms();
            raft.current_term = term;
            raft.role = NodeRole::Follower;
            raft.leader_id = Some(leader_id.clone());
            raft.leader_addr = Some(leader_addr.clone());
            raft.last_leader_contact_ms = now;

            {
                let mut cl = self.cluster.write().unwrap();
                cl.ring.add_node(&leader_id, &leader_addr);
                self.sync_has_peers(&cl);
                cl.sync_local_member(&self.cfg.node_id, false, NodeRole::Follower, term, now);
            }

            let mut to_apply: Vec<(u64, RaftCommand)> = Vec::new();
            for entry in entries {
                if entry.index > raft.last_log_index() {
                    to_apply.push((entry.index, entry.command.clone()));
                    raft.last_applied = entry.index;
                    raft.log.push(entry);
                }
            }
            let last_idx = raft.last_log_index();
            self.store.raft_log_idx.store(last_idx, Ordering::Release);
            if leader_commit > raft.commit_index {
                raft.commit_index = leader_commit.min(last_idx);
            }
            raft.maybe_compact();
            (raft.current_term, last_idx, to_apply)
        };

        for (ver, cmd) in to_apply {
            self.store.apply_raft_command(
                &self.state,
                &cmd,
                ver,
                self.cfg.max_entries,
                self.cfg.max_bytes,
            );
        }

        CacheResponse::RaftAppendReply {
            term: reply_term,
            success: true,
            match_index: match_idx,
        }
    }

    /// Trigger a Raft leader election across known cluster peers.
    pub fn run_leader_election(&self) -> bool {
        let (term, last_idx, last_term, peers, total_nodes) = {
            let mut raft = self.raft.lock().unwrap();
            self.sync_raft_atomic_progress(&mut raft);
            let now = unix_ms();
            raft.current_term += 1;
            raft.role = NodeRole::Candidate;
            raft.voted_for = Some(self.cfg.node_id.clone());
            let term = raft.current_term;
            let last_idx = raft.last_log_index();
            let last_term = raft.last_log_term();
            drop(raft);

            let mut cl = self.cluster.write().unwrap();
            cl.sync_local_member(&self.cfg.node_id, false, NodeRole::Candidate, term, now);
            let peers: Vec<String> = cl
                .members
                .values()
                .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                .map(|m| m.addr.clone())
                .collect();
            let total = cl.members.values().filter(|m| m.alive).count().max(1);
            (term, last_idx, last_term, peers, total)
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
                    let now = unix_ms();
                    {
                        let mut raft = self.raft.lock().unwrap();
                        raft.current_term = reply_term;
                        raft.role = NodeRole::Follower;
                        raft.voted_for = None;
                    }
                    let mut cl = self.cluster.write().unwrap();
                    cl.sync_local_member(
                        &self.cfg.node_id,
                        false,
                        NodeRole::Follower,
                        reply_term,
                        now,
                    );
                    return false;
                }
                if vote_granted {
                    votes += 1;
                }
            }
        }

        let won = votes >= quorum;
        if won {
            let mut became_leader = false;
            let now = unix_ms();
            {
                let mut raft = self.raft.lock().unwrap();
                if raft.current_term == term {
                    raft.role = NodeRole::Leader;
                    raft.leader_id = Some(self.cfg.node_id.clone());
                    raft.leader_addr = Some(self.cfg.listen_addr.clone());
                    became_leader = true;
                }
            }
            if became_leader {
                let mut cl = self.cluster.write().unwrap();
                cl.sync_local_member(&self.cfg.node_id, false, NodeRole::Leader, term, now);
            }
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
            let now = unix_ms();
            let (role, term) = {
                let raft = self.raft.lock().unwrap();
                (raft.role.clone(), raft.current_term)
            };
            let members: Vec<NodeMember> = {
                let mut cl = self.cluster.write().unwrap();
                cl.sync_local_member(&self.cfg.node_id, true, role, term, now);
                cl.members.values().cloned().collect()
            };
            let pubsub_events: Vec<PubSubEvent> = {
                let ps = self.pubsub.lock().unwrap();
                ps.recent_outbox.iter().cloned().collect()
            };
            CacheRequest::GossipSync {
                sender_id: self.cfg.node_id.clone(),
                sender_addr: self.cfg.listen_addr.clone(),
                members,
                pubsub_events,
            }
        };

        let resp = send_peer_rpc(peer_addr, &req)?;
        if let CacheResponse::GossipAck {
            node_id: _,
            members,
        } = resp
        {
            let mut cl = self.cluster.write().unwrap();
            let now = unix_ms();
            cl.merge_gossip_members(&self.cfg.node_id, members, now);
            self.sync_has_peers(&cl);
        }
        Ok(())
    }
}
