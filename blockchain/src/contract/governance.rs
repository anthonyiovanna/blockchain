use super::{Contract, ContractResult, ContractError, ContractEvent};
use super::standards::{
    GovernanceStandard,
    Proposal,
    ProposalState,
    ProposalCall,
    VoteType,
    VoteWeight,
    VoteReceipt,
    VoteCastEvent,
    ProposalCreatedEvent,
    ProposalExecutedEvent,
    DelegateChangedEvent,
    DelegateVotesChangedEvent,
    governance_storage_keys,
    governance_utils,
    token_utils,
};
use serde::{Serialize, Deserialize};

/// Configuration for governance contract
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceConfig {
    /// Delay before voting begins (in blocks)
    pub voting_delay: u64,
    /// Duration of voting period (in blocks)
    pub voting_period: u64,
    /// Minimum tokens required to create proposal
    pub proposal_threshold: u64,
    /// Minimum votes required for quorum
    pub quorum: u64,
    /// Delay before execution (in blocks)
    pub execution_delay: u64,
    /// Required signatures for execution
    pub required_signatures: u64,
}

/// Governance contract implementation
pub struct GovernanceContract {
    /// Base contract
    contract: Contract,
    /// Governance configuration
    config: GovernanceConfig,
    /// Token contract address for voting power
    token_address: [u8; 32],
}

impl GovernanceContract {
    /// Create new governance contract instance
    pub fn new(contract: Contract, config: GovernanceConfig, token_address: [u8; 32]) -> Self {
        Self {
            contract,
            config,
            token_address,
        }
    }

    /// Get current block number
    fn get_current_block(&self) -> ContractResult<u64> {
        // In a real implementation, this would get the current block from the blockchain
        Ok(0)
    }

    /// Get token balance of an account
    fn get_token_balance(&self, account: &[u8; 32]) -> ContractResult<u64> {
        // In a real implementation, this would call the token contract
        Ok(0)
    }

    /// Emit a contract event
    fn emit_event(&mut self, event: ContractEvent) -> ContractResult<()> {
        // In a real implementation, this would emit the event through the contract runtime
        Ok(())
    }

    /// Store proposal data
    fn store_proposal(&mut self, proposal: &Proposal) -> ContractResult<()> {
        let key = governance_storage_keys::proposal_key(proposal.id);
        let value = serde_json::to_vec(proposal)
            .map_err(|e| ContractError::ExecutionError(format!("Failed to serialize proposal: {}", e)))?;
        self.contract.set_state(key, value);
        Ok(())
    }

    /// Load proposal data
    fn load_proposal(&self, id: u64) -> ContractResult<Proposal> {
        let key = governance_storage_keys::proposal_key(id);
        let value = self.contract.get_state(&key)
            .ok_or_else(|| ContractError::NotFound(format!("Proposal {} not found", id)))?;
        
        serde_json::from_slice(value)
            .map_err(|e| ContractError::ExecutionError(format!("Failed to deserialize proposal: {}", e)))
    }

    /// Store vote receipt
    fn store_vote_receipt(&mut self, proposal_id: u64, voter: &[u8; 32], receipt: &VoteReceipt) -> ContractResult<()> {
        let key = governance_storage_keys::vote_receipt_key(proposal_id, voter);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| ContractError::ExecutionError(format!("Failed to serialize vote receipt: {}", e)))?;
        self.contract.set_state(key, value);
        Ok(())
    }

    /// Load vote receipt
    fn load_vote_receipt(&self, proposal_id: u64, voter: &[u8; 32]) -> ContractResult<VoteReceipt> {
        let key = governance_storage_keys::vote_receipt_key(proposal_id, voter);
        let value = self.contract.get_state(&key)
            .ok_or_else(|| ContractError::NotFound("Vote receipt not found".into()))?;
        
        serde_json::from_slice(value)
            .map_err(|e| ContractError::ExecutionError(format!("Failed to deserialize vote receipt: {}", e)))
    }

    /// Store delegate
    fn store_delegate(&mut self, account: &[u8; 32], delegate: &[u8; 32]) -> ContractResult<()> {
        let key = governance_storage_keys::delegate_key(account);
        self.contract.set_state(key, delegate.to_vec());
        Ok(())
    }

    /// Load delegate
    fn load_delegate(&self, account: &[u8; 32]) -> ContractResult<[u8; 32]> {
        let key = governance_storage_keys::delegate_key(account);
        let value = self.contract.get_state(&key)
            .unwrap_or_else(|| account.to_vec());
        
        let mut delegate = [0u8; 32];
        delegate.copy_from_slice(&value);
        Ok(delegate)
    }

    /// Store voting power at block
    fn store_voting_power(&mut self, account: &[u8; 32], block: u64, power: u64) -> ContractResult<()> {
        let key = governance_storage_keys::voting_power_key(account, block);
        self.contract.set_state(key, power.to_be_bytes().to_vec());
        Ok(())
    }

    /// Load voting power at block
    fn load_voting_power(&self, account: &[u8; 32], block: u64) -> ContractResult<u64> {
        let key = governance_storage_keys::voting_power_key(account, block);
        let value = self.contract.get_state(&key)
            .ok_or_else(|| ContractError::NotFound("Voting power not found".into()))?;
        
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&value);
        Ok(u64::from_be_bytes(bytes))
    }

    /// Execute proposal calls
    fn execute_calls(&mut self, calls: &[ProposalCall]) -> ContractResult<()> {
        for call in calls {
            // In a real implementation, this would execute the call through the contract runtime
            if call.target == [0u8; 32] {
                return Err(ContractError::ExecutionError("Invalid target address".into()));
            }
            if call.function.is_empty() {
                return Err(ContractError::ExecutionError("Empty function name".into()));
            }
        }
        Ok(())
    }

    /// Update vote counts
    fn update_vote_counts(&mut self, proposal: &mut Proposal, vote_type: VoteType, weight: u64) -> ContractResult<()> {
        match vote_type {
            VoteType::Against => {
                proposal.votes.against = token_utils::safe_add(proposal.votes.against, weight)?;
            },
            VoteType::For => {
                proposal.votes.for_votes = token_utils::safe_add(proposal.votes.for_votes, weight)?;
            },
            VoteType::Abstain => {
                proposal.votes.abstain = token_utils::safe_add(proposal.votes.abstain, weight)?;
            },
        }
        Ok(())
    }
}

impl GovernanceStandard for GovernanceContract {
    fn propose(&mut self, title: String, description: String, calls: Vec<ProposalCall>) -> ContractResult<u64> {
        let proposer_balance = self.get_token_balance(&self.contract.address)?;
        governance_utils::validate_proposal(
            proposer_balance,
            self.config.proposal_threshold,
            &title,
            &calls,
        )?;

        let current_block = self.get_current_block()?;
        let snapshot_block = current_block;
        let start_block = current_block + self.config.voting_delay;
        let end_block = start_block + self.config.voting_period;

        let proposal_count_key = governance_storage_keys::PROPOSAL_COUNT;
        let proposal_count = self.contract.get_state(proposal_count_key)
            .map(|v| u64::from_be_bytes(v.try_into().unwrap()))
            .unwrap_or(0);
        let proposal_id = proposal_count + 1;

        let proposal = Proposal {
            id: proposal_id,
            proposer: self.contract.address,
            title,
            description: description.clone(),
            calls,
            start_block,
            end_block,
            quorum: self.config.quorum,
            state: ProposalState::Pending,
            votes: VoteWeight {
                against: 0,
                for_votes: 0,
                abstain: 0,
            },
            created_at: current_block,
            execution_delay: self.config.execution_delay,
            required_signatures: self.config.required_signatures,
            snapshot_block,
        };

        self.store_proposal(&proposal)?;

        self.contract.set_state(
            proposal_count_key.to_vec(),
            proposal_id.to_be_bytes().to_vec(),
        );

        let event = ProposalCreatedEvent {
            proposal_id,
            proposer: self.contract.address,
            title: proposal.title,
            start_block,
            end_block,
            description,
            snapshot_block,
        };
        self.emit_event(ContractEvent::ProposalCreated(event))?;

        Ok(proposal_id)
    }

    fn cast_vote(&mut self, proposal_id: u64, vote_type: VoteType, reason: Option<String>) -> ContractResult<bool> {
        let mut proposal = self.load_proposal(proposal_id)?;
        let current_block = self.get_current_block()?;

        if !governance_utils::is_proposal_active(&proposal, current_block) {
            return Err(ContractError::ExecutionError("Proposal is not active".into()));
        }

        let has_voted = self.has_voted(proposal_id, &self.contract.address)?;
        let voting_power = self.get_voting_power(&self.contract.address, proposal.snapshot_block)?;
        
        governance_utils::validate_vote(vote_type, voting_power, has_voted)?;

        let receipt = VoteReceipt {
            has_voted: true,
            vote_type,
            weight: voting_power,
        };
        
        self.store_vote_receipt(proposal_id, &self.contract.address, &receipt)?;
        self.update_vote_counts(&mut proposal, vote_type, voting_power)?;
        self.store_proposal(&proposal)?;

        let event = VoteCastEvent {
            voter: self.contract.address,
            proposal_id,
            vote_type,
            weight: voting_power,
            reason,
        };
        self.emit_event(ContractEvent::VoteCast(event))?;

        Ok(true)
    }

    fn execute_proposal(&mut self, proposal_id: u64) -> ContractResult<bool> {
        let mut proposal = self.load_proposal(proposal_id)?;
        let current_block = self.get_current_block()?;

        if proposal.state != ProposalState::Queued {
            return Err(ContractError::ExecutionError("Proposal is not queued".into()));
        }

        let execution_time = proposal.end_block + proposal.execution_delay;
        if current_block < execution_time {
            return Err(ContractError::ExecutionError("Execution delay not met".into()));
        }

        self.execute_calls(&proposal.calls)?;

        proposal.state = ProposalState::Executed;
        self.store_proposal(&proposal)?;

        let event = ProposalExecutedEvent {
            proposal_id,
            executor: self.contract.address,
        };
        self.emit_event(ContractEvent::ProposalExecuted(event))?;

        Ok(true)
    }

    fn cancel_proposal(&mut self, proposal_id: u64) -> ContractResult<bool> {
        let mut proposal = self.load_proposal(proposal_id)?;

        let proposer_balance = self.get_token_balance(&proposal.proposer)?;
        if self.contract.address != proposal.proposer && proposer_balance >= self.config.proposal_threshold {
            return Err(ContractError::ExecutionError("Not authorized to cancel".into()));
        }

        if proposal.state == ProposalState::Executed {
            return Err(ContractError::ExecutionError("Cannot cancel executed proposal".into()));
        }

        proposal.state = ProposalState::Canceled;
        self.store_proposal(&proposal)?;

        Ok(true)
    }

    fn queue_proposal(&mut self, proposal_id: u64) -> ContractResult<bool> {
        let mut proposal = self.load_proposal(proposal_id)?;
        let current_block = self.get_current_block()?;

        if proposal.state != ProposalState::Active {
            return Err(ContractError::ExecutionError("Proposal is not active".into()));
        }

        if current_block <= proposal.end_block {
            return Err(ContractError::ExecutionError("Voting period not ended".into()));
        }

        if !governance_utils::has_proposal_succeeded(&proposal, self.config.quorum) {
            proposal.state = ProposalState::Defeated;
        } else {
            proposal.state = ProposalState::Queued;
        }

        self.store_proposal(&proposal)?;

        Ok(proposal.state == ProposalState::Queued)
    }

    fn get_proposal(&self, proposal_id: u64) -> ContractResult<Proposal> {
        self.load_proposal(proposal_id)
    }

    fn proposal_state(&self, proposal_id: u64) -> ContractResult<ProposalState> {
        let proposal = self.load_proposal(proposal_id)?;
        Ok(proposal.state)
    }

    fn get_voting_power(&self, account: &[u8; 32], block_number: u64) -> ContractResult<u64> {
        let delegate = self.load_delegate(account)?;
        if delegate == *account {
            self.get_token_balance(account)
        } else {
            self.load_voting_power(&delegate, block_number)
        }
    }

    fn has_voted(&self, proposal_id: u64, account: &[u8; 32]) -> ContractResult<bool> {
        Ok(self.load_vote_receipt(proposal_id, account)
            .map(|receipt| receipt.has_voted)
            .unwrap_or(false))
    }

    fn get_vote_receipt(&self, proposal_id: u64, account: &[u8; 32]) -> ContractResult<VoteReceipt> {
        self.load_vote_receipt(proposal_id, account)
    }

    fn delegate(&mut self, delegatee: &[u8; 32]) -> ContractResult<bool> {
        let current_delegate = self.load_delegate(&self.contract.address)?;
        if current_delegate == *delegatee {
            return Ok(false);
        }

        let current_block = self.get_current_block()?;
        let voting_power = self.get_token_balance(&self.contract.address)?;

        // Update old delegate's voting power
        if current_delegate != self.contract.address {
            let old_power = self.load_voting_power(&current_delegate, current_block)?;
            let new_power = token_utils::safe_sub(old_power, voting_power)?;
            self.store_voting_power(&current_delegate, current_block, new_power)?;

            self.emit_event(ContractEvent::DelegateVotesChanged(DelegateVotesChangedEvent {
                delegate: current_delegate,
                old_votes: old_power,
                new_votes: new_power,
            }))?;
        }

        // Update new delegate's voting power
        if *delegatee != self.contract.address {
            let old_power = self.load_voting_power(delegatee, current_block).unwrap_or(0);
            let new_power = token_utils::safe_add(old_power, voting_power)?;
            self.store_voting_power(delegatee, current_block, new_power)?;

            self.emit_event(ContractEvent::DelegateVotesChanged(DelegateVotesChangedEvent {
                delegate: *delegatee,
                old_votes: old_power,
                new_votes: new_power,
            }))?;
        }

        self.store_delegate(&self.contract.address, delegatee)?;

        self.emit_event(ContractEvent::DelegateChanged(DelegateChangedEvent {
            delegator: self.contract.address,
            from_delegate: current_delegate,
            to_delegate: *delegatee,
        }))?;

        Ok(true)
    }

    fn delegates(&self, account: &[u8; 32]) -> ContractResult<[u8; 32]> {
        self.load_delegate(account)
    }

    fn get_past_voting_power(&self, account: &[u8; 32], block_number: u64) -> ContractResult<u64> {
        let current_block = self.get_current_block()?;
        if block_number >= current_block {
            return Err(ContractError::ExecutionError("Block number is in the future".into()));
        }
        
        let delegate = self.load_delegate(account)?;
        if delegate == *account {
            // In a real implementation, this would get historical token balance
            self.get_token_balance(account)
        } else {
            self.load_voting_power(&delegate, block_number)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_contract() -> GovernanceContract {
        let contract = Contract::new(
            [0u8; 32],
            vec![],
            super::super::ContractABI {
                methods: vec![],
                events: vec![],
                standards: vec!["Governance".to_string()],
            },
            super::super::ResourceLimits {
                max_memory: 1024 * 1024,
                max_gas: 1_000_000,
                max_storage: 1024 * 1024,
                max_call_depth: 5,
            },
        );

        let config = GovernanceConfig {
            voting_delay: 1,
            voting_period: 10,
            proposal_threshold: 100,
            quorum: 1000,
            execution_delay: 2,
            required_signatures: 1,
        };

        GovernanceContract::new(contract, config, [1u8; 32])
    }

    #[test]
    fn test_proposal_lifecycle() {
        let mut gov = create_test_contract();
        
        let calls = vec![ProposalCall {
            target: [1u8; 32],
            function: "test".to_string(),
            args: vec![],
        }];
        
        let proposal_id = gov.propose(
            "Test Proposal".to_string(),
            "Description".to_string(),
            calls,
        ).unwrap();

        let proposal = gov.get_proposal(proposal_id).unwrap();
        assert_eq!(proposal.state, ProposalState::Pending);
        assert_eq!(proposal.title, "Test Proposal");
    }

    #[test]
    fn test_voting() {
        let mut gov = create_test_contract();
        
        let proposal_id = gov.propose(
            "Test Proposal".to_string(),
            "Description".to_string(),
            vec![],
        ).unwrap();

        let result = gov.cast_vote(proposal_id, VoteType::For, None);
        assert!(result.is_ok());

        assert!(gov.has_voted(proposal_id, &gov.contract.address).unwrap());
        
        let receipt = gov.get_vote_receipt(proposal_id, &gov.contract.address).unwrap();
        assert_eq!(receipt.vote_type, VoteType::For);
    }

    #[test]
    fn test_delegation() {
        let mut gov = create_test_contract();
        let delegatee = [2u8; 32];

        let result = gov.delegate(&delegatee);
        assert!(result.is_ok());

        let current_delegate = gov.delegates(&gov.contract.address).unwrap();
        assert_eq!(current_delegate, delegatee);
    }
}
