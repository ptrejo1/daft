use crate::node::config::Address;
use crate::node::peer_client::PeerClient;
use anyhow::Result;
use crate::node::raft_rpc::{AppendEntriesReply, VoteReply};


#[derive(Debug)]
pub struct Peer {
    address: Address,
    peer_client: PeerClient,
}

impl Peer {

    pub async fn new(address: Address) -> Result<Self> {
        let peer_client = PeerClient::new(&address).await?;
        Ok(Self { address, peer_client })
    }

    pub async fn request_vote(&self, timeout: u64, term: u32, candidate_id: String) -> Result<VoteReply> {
        // todo: fill in log term and index
        let reply = self.peer_client.request_vote(
            timeout, term, candidate_id, 0, 0
        ).await?;

        Ok(reply)
    }

    pub async fn append_entries(&self, timeout: u64, term: u32, leader_id: String) -> Result<AppendEntriesReply> {
        let reply = self.peer_client.append_entries(
            timeout, term, leader_id, 0, 0, vec![], 0
        ).await?;

        Ok(reply)
    }
}
