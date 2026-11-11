use crate::error::Error;

use super::client::send_rpc;
use super::cluster::{NodeMember, NodeRole, PubSubEvent, RaftCommand, RaftLogEntry};
use super::protocol::{CacheRequest, CacheResponse};
use super::server::CacheNode;
use super::unix_ms;

impl CacheNode {
    pub(crate) fn propose_and_apply_mutation(&self, cmd: RaftCommand) {
        let (entry, prev_idx, prev_term, term, peers) = {
            let mut st = self.state.lock().unwrap();
            let prev_idx = st.raft.last_log_index();
            let prev_term = st.raft.last_log_term();
            let term = st.raft.current_term;
            let idx = prev_idx + 1;
            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd.clone(),
            };
            st.raft.log.push(entry.clone());
            st.apply_raft_command(&cmd, self.cfg.max_entries, self.cfg.max_bytes);
            st.raft.commit_index = idx;
            st.raft.last_applied = idx;
            st.raft.maybe_compact();

            let peers: Vec<String> = if self.cfg.raft_replication {
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                    .map(|m| m.addr.clone())
                    .collect()
            } else {
                Vec::new()
            };
            (entry, prev_idx, prev_term, term, peers)
        };

        if !peers.is_empty() {
            let rpc = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: prev_idx,
                prev_log_term: prev_term,
                entries: vec![entry],
                leader_commit: prev_idx + 1,
            };
            for peer_addr in peers {
                let _ = send_rpc(&peer_addr, &rpc);
            }
        }
    }

    pub(crate) fn propose_raft_only(&self, cmd: RaftCommand) {
        let (entry, prev_idx, prev_term, term, peers) = {
            let mut st = self.state.lock().unwrap();
            let prev_idx = st.raft.last_log_index();
            let prev_term = st.raft.last_log_term();
            let term = st.raft.current_term;
            let idx = prev_idx + 1;
            let entry = RaftLogEntry {
                index: idx,
                term,
                command: cmd,
            };
            st.raft.log.push(entry.clone());
            st.raft.commit_index = idx;
            st.raft.last_applied = idx;
            st.raft.maybe_compact();
            let peers: Vec<String> = if self.cfg.raft_replication {
                st.members
                    .values()
                    .filter(|m| m.node_id != self.cfg.node_id && m.alive)
                    .map(|m| m.addr.clone())
                    .collect()
            } else {
                Vec::new()
            };
            (entry, prev_idx, prev_term, term, peers)
        };

        if !peers.is_empty() {
            let rpc = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: prev_idx,
                prev_log_term: prev_term,
                entries: vec![entry],
                leader_commit: prev_idx + 1,
            };
            for peer_addr in peers {
                let _ = send_rpc(&peer_addr, &rpc);
            }
        }
    }

    pub(crate) fn handle_gossip_sync(
        &self,
        sender_id: String,
        sender_addr: String,
        incoming_members: Vec<NodeMember>,
        pubsub_events: Vec<PubSubEvent>,
    ) -> CacheResponse {
        let mut st = self.state.lock().unwrap();
        let now = unix_ms();

        // Ensure sender is in the ring & membership table
        st.ring.add_node(&sender_id, &sender_addr);
        for inc in incoming_members {
            if inc.node_id == self.cfg.node_id {
                continue;
            }
            st.ring.add_node(&inc.node_id, &inc.addr);
            st.members
                .entry(inc.node_id.clone())
                .and_modify(|m| {
                    if inc.heartbeat >= m.heartbeat {
                        m.heartbeat = inc.heartbeat;
                        m.addr = inc.addr.clone();
                        m.role = inc.role.clone();
                        m.term = inc.term;
                        m.alive = true;
                        m.last_seen_ms = now;
                    }
                })
                .or_insert_with(|| {
                    let mut m = inc;
                    m.last_seen_ms = now;
                    m
                });
        }

        // Process incoming Pub/Sub events
        for ev in pubsub_events {
            if st.pubsub.mark_seen(ev.id) {
                st.publish_local(&ev.channel, &ev.payload, &ev.origin_node);
            }
        }

        // Update our own heartbeat in membership table
        st.heartbeat += 1;
        let hb = st.heartbeat;
        let role = st.raft.role.clone();
        let term = st.raft.current_term;
        if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
            me.heartbeat = hb;
            me.last_seen_ms = now;
            me.alive = true;
            me.role = role;
            me.term = term;
        }

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
        let mut st = self.state.lock().unwrap();
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
            st.raft.voted_for = Some(candidate_id.clone());
            st.raft.last_leader_contact_ms = unix_ms();
            let cur_term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.role = NodeRole::Follower;
                me.term = cur_term;
            }
            CacheResponse::RaftVoteReply {
                term: cur_term,
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
        let mut st = self.state.lock().unwrap();
        if term < st.raft.current_term {
            return CacheResponse::RaftAppendReply {
                term: st.raft.current_term,
                success: false,
                match_index: st.raft.last_log_index(),
            };
        }

        st.raft.current_term = term;
        st.raft.role = NodeRole::Follower;
        st.raft.leader_id = Some(leader_id.clone());
        st.raft.leader_addr = Some(leader_addr.clone());
        st.raft.last_leader_contact_ms = unix_ms();
        st.ring.add_node(&leader_id, &leader_addr);

        if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
            me.role = NodeRole::Follower;
            me.term = term;
        }

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
            let mut st = self.state.lock().unwrap();
            st.raft.current_term += 1;
            st.raft.role = NodeRole::Candidate;
            st.raft.voted_for = Some(self.cfg.node_id.clone());
            let term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.role = NodeRole::Candidate;
                me.term = term;
            }
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

        for peer_addr in &peers {
            if let Ok(CacheResponse::RaftVoteReply {
                term: reply_term,
                vote_granted,
            }) = send_rpc(peer_addr, &req)
            {
                if reply_term > term {
                    let mut st = self.state.lock().unwrap();
                    st.raft.current_term = reply_term;
                    st.raft.role = NodeRole::Follower;
                    st.raft.voted_for = None;
                    return false;
                }
                if vote_granted {
                    votes += 1;
                }
            }
        }

        let won = votes >= quorum;
        if won {
            let mut st = self.state.lock().unwrap();
            if st.raft.current_term == term {
                st.raft.role = NodeRole::Leader;
                st.raft.leader_id = Some(self.cfg.node_id.clone());
                st.raft.leader_addr = Some(self.cfg.listen_addr.clone());
                if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                    me.role = NodeRole::Leader;
                    me.term = term;
                }
            }
            drop(st);
            // Send immediate heartbeat AppendEntries to establish leadership
            let hb = CacheRequest::RaftAppendEntries {
                term,
                leader_id: self.cfg.node_id.clone(),
                leader_addr: self.cfg.listen_addr.clone(),
                prev_log_index: last_idx,
                prev_log_term: last_term,
                entries: Vec::new(),
                leader_commit: last_idx,
            };
            for peer_addr in &peers {
                let _ = send_rpc(peer_addr, &hb);
            }
        }
        won
    }

    /// Perform one Gossip round with a specific peer address.
    pub fn gossip_once_with(&self, peer_addr: &str) -> Result<(), Error> {
        let req = {
            let mut st = self.state.lock().unwrap();
            st.heartbeat += 1;
            let hb = st.heartbeat;
            let now = unix_ms();
            let role = st.raft.role.clone();
            let term = st.raft.current_term;
            if let Some(me) = st.members.get_mut(&self.cfg.node_id) {
                me.heartbeat = hb;
                me.last_seen_ms = now;
                me.alive = true;
                me.role = role;
                me.term = term;
            }
            CacheRequest::GossipSync {
                sender_id: self.cfg.node_id.clone(),
                sender_addr: self.cfg.listen_addr.clone(),
                members: st.members.values().cloned().collect(),
                pubsub_events: st.pubsub.recent_outbox.iter().cloned().collect(),
            }
        };

        let resp = send_rpc(peer_addr, &req)?;
        if let CacheResponse::GossipAck {
            node_id: _,
            members,
        } = resp
        {
            let mut st = self.state.lock().unwrap();
            let now = unix_ms();
            for inc in members {
                if inc.node_id == self.cfg.node_id {
                    continue;
                }
                st.ring.add_node(&inc.node_id, &inc.addr);
                st.members
                    .entry(inc.node_id.clone())
                    .and_modify(|m| {
                        if inc.heartbeat >= m.heartbeat {
                            m.heartbeat = inc.heartbeat;
                            m.addr = inc.addr.clone();
                            m.role = inc.role.clone();
                            m.term = inc.term;
                            m.alive = true;
                            m.last_seen_ms = now;
                        }
                    })
                    .or_insert_with(|| {
                        let mut m = inc;
                        m.last_seen_ms = now;
                        m
                    });
            }
        }
        Ok(())
    }
}
