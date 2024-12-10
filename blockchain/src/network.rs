use libp2p::{
    core::transport::upgrade,
    gossipsub::{
        self, IdentTopic as Topic, MessageAuthenticity, ValidationMode,
    },
    identity, noise, yamux,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, Multiaddr, PeerId, Transport, Swarm,
};
use std::error::Error;
use std::time::Duration;
use tokio::sync::mpsc;
use futures::StreamExt;
use std::collections::{HashMap, HashSet};
use serde::{Serialize, Deserialize};
use std::fmt;
use crate::block::Block;

// Custom error type for network operations
#[derive(Debug)]
pub enum NetworkError {
    BlockValidation(String),
    SyncError(String),
    PartitionError(String),
    PropagationError(String),
}

impl fmt::Display for NetworkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkError::BlockValidation(msg) => write!(f, "Block validation error: {}", msg),
            NetworkError::SyncError(msg) => write!(f, "Sync error: {}", msg),
            NetworkError::PartitionError(msg) => write!(f, "Partition error: {}", msg),
            NetworkError::PropagationError(msg) => write!(f, "Propagation error: {}", msg),
        }
    }
}

// Block synchronization message types
#[derive(Debug, Serialize, Deserialize)]
pub enum SyncMessage {
    BlockRequest { start: u64, end: u64 },
    BlockResponse { blocks: Vec<Block> },
    ChainHeight { height: u64 },
}

// Custom event type for our network behavior
#[derive(Debug)]
pub enum NetworkEvent {
    GossipMessage(gossipsub::Event),
    BlockReceived(Block),
    SyncStarted,
    SyncCompleted,
    PartitionDetected,
    PartitionResolved,
}

impl From<gossipsub::Event> for NetworkEvent {
    fn from(event: gossipsub::Event) -> Self {
        NetworkEvent::GossipMessage(event)
    }
}

// Network behavior implementation
#[derive(NetworkBehaviour)]
#[behaviour(out_event = "NetworkEvent")]
pub struct BlockchainBehaviour {
    gossipsub: gossipsub::Behaviour,
}

pub struct Network {
    swarm: Swarm<BlockchainBehaviour>,
    _events_sender: mpsc::UnboundedSender<NetworkEvent>,
    peers: HashMap<PeerId, PeerInfo>,
    known_blocks: HashSet<String>, // Block hashes we've seen
    sync_state: SyncState,
}

#[derive(Debug)]
struct PeerInfo {
    chain_height: u64,
    last_seen: std::time::Instant,
    sync_score: f64,
}

#[derive(Debug)]
struct SyncState {
    is_syncing: bool,
    target_height: u64,
    current_height: u64,
    pending_requests: HashSet<u64>,
}

impl Network {
    pub async fn new(
        events_sender: mpsc::UnboundedSender<NetworkEvent>,
    ) -> Result<Self, Box<dyn Error>> {
        // Create a random PeerId
        let id_keys = identity::Keypair::generate_ed25519();
        let peer_id = PeerId::from(id_keys.public());
        println!("Local peer id: {:?}", peer_id);

        // Create a transport
        let transport = tcp::tokio::Transport::new(tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1)
            .authenticate(noise::Config::new(&id_keys)?)
            .multiplex(yamux::Config::default())
            .boxed();

        // Create a Gossipsub configuration
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(1))
            .validation_mode(ValidationMode::Strict)
            .build()
            .expect("Valid config");

        // Build a gossipsub network behaviour
        let gossipsub = gossipsub::Behaviour::new(
            MessageAuthenticity::Signed(id_keys),
            gossipsub_config,
        ).expect("Correct configuration");

        // Create a Swarm to manage peers and events
        let behaviour = BlockchainBehaviour {
            gossipsub,
        };

        let config = libp2p::swarm::Config::with_tokio_executor();
        let swarm = Swarm::new(transport, behaviour, peer_id, config);

        Ok(Network {
            swarm,
            _events_sender: events_sender,
            peers: HashMap::new(),
            known_blocks: HashSet::new(),
            sync_state: SyncState {
                is_syncing: false,
                target_height: 0,
                current_height: 0,
                pending_requests: HashSet::new(),
            },
        })
    }

    pub async fn broadcast_block(&mut self, block: Block) -> Result<(), NetworkError> {
        // Validate block before broadcasting
        if !self.validate_block(&block) {
            return Err(NetworkError::BlockValidation("Invalid block".to_string()));
        }

        // Serialize block
        let block_data = serde_json::to_vec(&block)
            .map_err(|e| NetworkError::PropagationError(e.to_string()))?;

        // Broadcast to all peers
        let topic = Topic::new("blocks");
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, block_data)
            .map_err(|e| NetworkError::PropagationError(e.to_string()))?;

        // Add to known blocks
        self.known_blocks.insert(block.hash.to_string());

        Ok(())
    }

    pub async fn sync_blocks(&mut self) -> Result<(), NetworkError> {
        if self.sync_state.is_syncing {
            return Ok(());
        }

        // Find highest chain among peers
        let target_height = self.get_network_height();
        if target_height <= self.sync_state.current_height {
            return Ok(());
        }

        self.sync_state.is_syncing = true;
        self.sync_state.target_height = target_height;
        self._events_sender.send(NetworkEvent::SyncStarted)
            .map_err(|e| NetworkError::SyncError(e.to_string()))?;

        // Request blocks in batches
        let batch_size = 50;
        let mut start = self.sync_state.current_height + 1;

        while start <= target_height {
            let end = std::cmp::min(start + batch_size - 1, target_height);
            self.request_blocks(start, end).await?;
            start = end + 1;
        }

        Ok(())
    }

    pub async fn handle_network_partition(&mut self) -> Result<(), NetworkError> {
        // Check for network partition
        if self.detect_partition() {
            self._events_sender.send(NetworkEvent::PartitionDetected)
                .map_err(|e| NetworkError::PartitionError(e.to_string()))?;

            // Implement recovery mechanism
            self.initiate_partition_recovery().await?;
        }

        Ok(())
    }

    // Helper methods
    fn validate_block(&self, block: &Block) -> bool {
        // Implement block validation logic
        // This is a placeholder - actual implementation would be more comprehensive
        !self.known_blocks.contains(&block.hash.to_string())
    }

    fn get_network_height(&self) -> u64 {
        self.peers.values()
            .map(|info| info.chain_height)
            .max()
            .unwrap_or(0)
    }

    async fn request_blocks(&mut self, start: u64, end: u64) -> Result<(), NetworkError> {
        let msg = SyncMessage::BlockRequest { start, end };
        let data = serde_json::to_vec(&msg)
            .map_err(|e| NetworkError::SyncError(e.to_string()))?;

        // Send request to best peer
        let topic = Topic::new("sync");
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)
            .map_err(|e| NetworkError::SyncError(e.to_string()))?;

        // Mark blocks as pending
        for block_num in start..=end {
            self.sync_state.pending_requests.insert(block_num);
        }

        Ok(())
    }

    fn detect_partition(&self) -> bool {
        let now = std::time::Instant::now();
        let active_peers = self.peers.values()
            .filter(|info| now.duration_since(info.last_seen) < Duration::from_secs(30))
            .count();

        // Consider it a partition if we have less than 3 active peers
        active_peers < 3
    }

    async fn initiate_partition_recovery(&mut self) -> Result<(), NetworkError> {
        // Implement partition recovery logic
        // 1. Store current chain state
        // 2. Attempt to reconnect to known peers
        // 3. Request chain state from connected peers
        // 4. Resolve conflicts and restore consistency

        // This is a placeholder implementation
        if let Err(e) = self.sync_blocks().await {
            return Err(NetworkError::PartitionError(format!("Recovery failed: {}", e)));
        }

        self._events_sender.send(NetworkEvent::PartitionResolved)
            .map_err(|e| NetworkError::PartitionError(e.to_string()))?;

        Ok(())
    }

    pub async fn start_listening(&mut self, addr: Multiaddr) -> Result<(), Box<dyn Error>> {
        self.swarm.listen_on(addr)?;
        Ok(())
    }

    pub async fn dial_peer(&mut self, addr: Multiaddr) -> Result<(), Box<dyn Error>> {
        self.swarm.dial(addr)?;
        Ok(())
    }

    pub async fn publish(&mut self, topic: &str, data: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let topic = Topic::new(topic);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .publish(topic, data)?;
        Ok(())
    }

    pub async fn subscribe(&mut self, topic: &str) -> Result<(), Box<dyn Error>> {
        let topic = Topic::new(topic);
        self.swarm
            .behaviour_mut()
            .gossipsub
            .subscribe(&topic)?;
        Ok(())
    }

    pub async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => {
                    if let SwarmEvent::Behaviour(event) = event {
                        match event {
                            NetworkEvent::GossipMessage(gossip_event) => {
                                match gossip_event {
                                    gossipsub::Event::Message { 
                                        message: gossipsub::Message { data, source, .. },
                                        ..
                                    } => {
                                        // Handle different message types
                                        if let Ok(sync_msg) = serde_json::from_slice::<SyncMessage>(&data) {
                                            match sync_msg {
                                                SyncMessage::BlockRequest { start, end } => {
                                                    // Handle block request
                                                    println!("Received block request: {} to {}", start, end);
                                                }
                                                SyncMessage::BlockResponse { blocks } => {
                                                    // Process received blocks
                                                    for block in blocks {
                                                        if self.validate_block(&block) {
                                                            self._events_sender.send(NetworkEvent::BlockReceived(block))
                                                                .expect("Event channel should be open");
                                                        }
                                                    }
                                                }
                                                SyncMessage::ChainHeight { height } => {
                                                    // Update peer's chain height
                                                    if let Some(peer_id) = source {
                                                        if let Some(peer_info) = self.peers.get_mut(&peer_id) {
                                                            peer_info.chain_height = height;
                                                            peer_info.last_seen = std::time::Instant::now();
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    _ => {} // Handle other gossipsub events if needed
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ = interval.tick() => {
                    // Periodic tasks
                    if let Err(e) = self.handle_network_partition().await {
                        println!("Error handling network partition: {:?}", e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc::unbounded_channel;

    #[tokio::test]
    async fn test_network_creation() {
        let (sender, _receiver) = unbounded_channel();
        let network = Network::new(sender).await;
        assert!(network.is_ok());
    }

    #[tokio::test]
    async fn test_block_broadcast() {
        let (sender, _receiver) = unbounded_channel();
        let mut network = Network::new(sender).await.unwrap();
        
        // Create a test block
        let block = Block::default();
        let result = network.broadcast_block(block).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_sync_blocks() {
        let (sender, _receiver) = unbounded_channel();
        let mut network = Network::new(sender).await.unwrap();
        
        let result = network.sync_blocks().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_partition_detection() {
        let (sender, _receiver) = unbounded_channel();
        let network = Network::new(sender).await.unwrap();
        
        assert!(!network.detect_partition());
    }
}
