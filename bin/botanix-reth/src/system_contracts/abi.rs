use lazy_static::lazy_static;

lazy_static! {
    pub static ref VALIDATOR_SET_ABI: &'static str = r#"
    [
  {
    "type": "receive",
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "BC_FUSION_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "BIND_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "BLOCK_FEES_RATIO_SCALE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "BURN_ADDRESS",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "CODE_OK",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "CROSS_CHAIN_CONTRACT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "CROSS_STAKE_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "DUSTY_INCOMING",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "ERROR_FAIL_CHECK_VALIDATORS",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "ERROR_FAIL_DECODE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "ERROR_LEN_OF_VAL_MISMATCH",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "ERROR_RELAYFEE_TOO_LARGE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "ERROR_UNKNOWN_PACKAGE_TYPE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint32",
        "internalType": "uint32"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "EXPIRE_TIME_SECOND_GAP",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "GOVERNOR_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "GOV_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "GOV_HUB_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "GOV_TOKEN_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INCENTIVIZE_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_BURN_RATIO",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_MAINTAIN_SLASH_SCALE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_MAX_NUM_OF_MAINTAINING",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_NUM_OF_CABINETS",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_SYSTEM_REWARD_RATIO",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "INIT_VALIDATORSET_BYTES",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "JAIL_MESSAGE_TYPE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "LIGHT_CLIENT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "MAX_NUM_OF_VALIDATORS",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "MAX_SYSTEM_REWARD_BALANCE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "PRECISION",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "RELAYERHUB_CONTRACT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "SLASH_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "SLASH_CONTRACT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "STAKE_CREDIT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "STAKE_HUB_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "STAKING_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "STAKING_CONTRACT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "SYSTEM_REWARD_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TIMELOCK_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TOKEN_HUB_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TOKEN_MANAGER_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TOKEN_RECOVER_PORTAL_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TRANSFER_IN_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "TRANSFER_OUT_CHANNELID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "VALIDATORS_UPDATE_MESSAGE_TYPE",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "VALIDATOR_CONTRACT_ADDR",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "alreadyInit",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "bscChainID",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint16",
        "internalType": "uint16"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "burnRatio",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "burnRatioInitialized",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "canEnterMaintenance",
    "inputs": [
      {
        "name": "index",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "currentValidatorSet",
    "inputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "consensusAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "feeAddress",
        "type": "address",
        "internalType": "address payable"
      },
      {
        "name": "BBCFeeAddress",
        "type": "address",
        "internalType": "address"
      },
      {
        "name": "votingPower",
        "type": "uint64",
        "internalType": "uint64"
      },
      {
        "name": "jailed",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "incoming",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "currentValidatorSetMap",
    "inputs": [
      {
        "name": "",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "currentVoteAddrFullSet",
    "inputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "deposit",
    "inputs": [
      {
        "name": "valAddr",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "payable"
  },
  {
    "type": "function",
    "name": "distributeFinalityReward",
    "inputs": [
      {
        "name": "valAddrs",
        "type": "address[]",
        "internalType": "address[]"
      },
      {
        "name": "weights",
        "type": "uint256[]",
        "internalType": "uint256[]"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "enterMaintenance",
    "inputs": [],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "exitMaintenance",
    "inputs": [],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "expireTimeSecondGap",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "felony",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "getCurrentValidatorIndex",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getIncoming",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getLivingValidators",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address[]",
        "internalType": "address[]"
      },
      {
        "name": "",
        "type": "bytes[]",
        "internalType": "bytes[]"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getMiningValidators",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address[]",
        "internalType": "address[]"
      },
      {
        "name": "",
        "type": "bytes[]",
        "internalType": "bytes[]"
      }
    ],
    "stateMutability": "view"
  },
  {
    "inputs": [],
    "name": "getTurnLength",
    "outputs": [
      {
        "internalType": "uint256",
        "name": "",
        "type": "uint256"
      }
    ],
    "stateMutability": "view",
    "type": "function"
  },
  {
    "type": "function",
    "name": "getValidators",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "address[]",
        "internalType": "address[]"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "getWorkingValidatorCount",
    "inputs": [],
    "outputs": [
      {
        "name": "workingValidatorCount",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "handleAckPackage",
    "inputs": [
      {
        "name": "channelId",
        "type": "uint8",
        "internalType": "uint8"
      },
      {
        "name": "msgBytes",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "handleFailAckPackage",
    "inputs": [
      {
        "name": "channelId",
        "type": "uint8",
        "internalType": "uint8"
      },
      {
        "name": "msgBytes",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "handleSynPackage",
    "inputs": [
      {
        "name": "",
        "type": "uint8",
        "internalType": "uint8"
      },
      {
        "name": "msgBytes",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [
      {
        "name": "responsePayload",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "init",
    "inputs": [],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "isCurrentValidator",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "isMonitoredForMaliciousVote",
    "inputs": [
      {
        "name": "voteAddr",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "isSystemRewardIncluded",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "isWorkingValidator",
    "inputs": [
      {
        "name": "index",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bool",
        "internalType": "bool"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "maintainSlashScale",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "maxNumOfCandidates",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "maxNumOfMaintaining",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "maxNumOfWorkingCandidates",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "misdemeanor",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "numOfCabinets",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "numOfJailed",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "numOfMaintaining",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "previousBalanceOfSystemReward",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "previousHeight",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "previousVoteAddrFullSet",
    "inputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "removeTmpMigratedValidator",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "internalType": "address"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "systemRewardRatio",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "totalInComing",
    "inputs": [],
    "outputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "function",
    "name": "updateParam",
    "inputs": [
      {
        "name": "key",
        "type": "string",
        "internalType": "string"
      },
      {
        "name": "value",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "updateValidatorSetV2",
    "inputs": [
      {
        "name": "_consensusAddrs",
        "type": "address[]",
        "internalType": "address[]"
      },
      {
        "name": "_votingPowers",
        "type": "uint64[]",
        "internalType": "uint64[]"
      },
      {
        "name": "_voteAddrs",
        "type": "bytes[]",
        "internalType": "bytes[]"
      }
    ],
    "outputs": [],
    "stateMutability": "nonpayable"
  },
  {
    "type": "function",
    "name": "validatorExtraSet",
    "inputs": [
      {
        "name": "",
        "type": "uint256",
        "internalType": "uint256"
      }
    ],
    "outputs": [
      {
        "name": "enterMaintenanceHeight",
        "type": "uint256",
        "internalType": "uint256"
      },
      {
        "name": "isMaintaining",
        "type": "bool",
        "internalType": "bool"
      },
      {
        "name": "voteAddress",
        "type": "bytes",
        "internalType": "bytes"
      }
    ],
    "stateMutability": "view"
  },
  {
    "type": "event",
    "name": "batchTransfer",
    "inputs": [
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "batchTransferFailed",
    "inputs": [
      {
        "name": "amount",
        "type": "uint256",
        "indexed": true,
        "internalType": "uint256"
      },
      {
        "name": "reason",
        "type": "string",
        "indexed": false,
        "internalType": "string"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "batchTransferLowerFailed",
    "inputs": [
      {
        "name": "amount",
        "type": "uint256",
        "indexed": true,
        "internalType": "uint256"
      },
      {
        "name": "reason",
        "type": "bytes",
        "indexed": false,
        "internalType": "bytes"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "deprecatedDeposit",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "deprecatedFinalityRewardDeposit",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "directTransfer",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address payable"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "directTransferFail",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address payable"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "failReasonWithStr",
    "inputs": [
      {
        "name": "message",
        "type": "string",
        "indexed": false,
        "internalType": "string"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "feeBurned",
    "inputs": [
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "finalityRewardDeposit",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "paramChange",
    "inputs": [
      {
        "name": "key",
        "type": "string",
        "indexed": false,
        "internalType": "string"
      },
      {
        "name": "value",
        "type": "bytes",
        "indexed": false,
        "internalType": "bytes"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "systemTransfer",
    "inputs": [
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "tmpValidatorSetUpdated",
    "inputs": [
      {
        "name": "validatorsNum",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "unexpectedPackage",
    "inputs": [
      {
        "name": "channelId",
        "type": "uint8",
        "indexed": false,
        "internalType": "uint8"
      },
      {
        "name": "msgBytes",
        "type": "bytes",
        "indexed": false,
        "internalType": "bytes"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorDeposit",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorEmptyJailed",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorEnterMaintenance",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorExitMaintenance",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorFelony",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorJailed",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorMisdemeanor",
    "inputs": [
      {
        "name": "validator",
        "type": "address",
        "indexed": true,
        "internalType": "address"
      },
      {
        "name": "amount",
        "type": "uint256",
        "indexed": false,
        "internalType": "uint256"
      }
    ],
    "anonymous": false
  },
  {
    "type": "event",
    "name": "validatorSetUpdated",
    "inputs": [],
    "anonymous": false
  }
]"#;
    pub static ref STAKE_HUB_ABI: &'static str = r#"
        [
    {
        "type": "receive",
        "stateMutability": "payable"
    },
    {
        "type": "function",
        "name": "BREATHE_BLOCK_INTERVAL",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "DEAD_ADDRESS",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "INIT_MAX_NUMBER_NODE_ID",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "LOCK_AMOUNT",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "REDELEGATE_FEE_RATE_BASE",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "addNodeIDs",
        "inputs": [
            {
                "name": "nodeIDs",
                "type": "bytes32[]",
                "internalType": "bytes32[]"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "addToBlackList",
        "inputs": [
            {
                "name": "account",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "agentToOperator",
        "inputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "blackList",
        "inputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "bool",
                "internalType": "bool"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "claim",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "requestNumber",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "claimBatch",
        "inputs": [
            {
                "name": "operatorAddresses",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "requestNumbers",
                "type": "uint256[]",
                "internalType": "uint256[]"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "consensusExpiration",
        "inputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "consensusToOperator",
        "inputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "createValidator",
        "inputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "voteAddress",
                "type": "bytes",
                "internalType": "bytes"
            },
            {
                "name": "blsProof",
                "type": "bytes",
                "internalType": "bytes"
            },
            {
                "name": "commission",
                "type": "tuple",
                "internalType": "struct StakeHub.Commission",
                "components": [
                    {
                        "name": "rate",
                        "type": "uint64",
                        "internalType": "uint64"
                    },
                    {
                        "name": "maxRate",
                        "type": "uint64",
                        "internalType": "uint64"
                    },
                    {
                        "name": "maxChangeRate",
                        "type": "uint64",
                        "internalType": "uint64"
                    }
                ]
            },
            {
                "name": "description",
                "type": "tuple",
                "internalType": "struct StakeHub.Description",
                "components": [
                    {
                        "name": "moniker",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "identity",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "website",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "details",
                        "type": "string",
                        "internalType": "string"
                    }
                ]
            }
        ],
        "outputs": [],
        "stateMutability": "payable"
    },
    {
        "type": "function",
        "name": "delegate",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "delegateVotePower",
                "type": "bool",
                "internalType": "bool"
            }
        ],
        "outputs": [],
        "stateMutability": "payable"
    },
    {
        "type": "function",
        "name": "distributeReward",
        "inputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "payable"
    },
    {
        "type": "function",
        "name": "doubleSignSlash",
        "inputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "downtimeJailTime",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "downtimeSlash",
        "inputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "downtimeSlashAmount",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "editCommissionRate",
        "inputs": [
            {
                "name": "commissionRate",
                "type": "uint64",
                "internalType": "uint64"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "editConsensusAddress",
        "inputs": [
            {
                "name": "newConsensusAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "editDescription",
        "inputs": [
            {
                "name": "description",
                "type": "tuple",
                "internalType": "struct StakeHub.Description",
                "components": [
                    {
                        "name": "moniker",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "identity",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "website",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "details",
                        "type": "string",
                        "internalType": "string"
                    }
                ]
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "editVoteAddress",
        "inputs": [
            {
                "name": "newVoteAddress",
                "type": "bytes",
                "internalType": "bytes"
            },
            {
                "name": "blsProof",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "felonyJailTime",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "felonySlashAmount",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getProtector",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorAgent",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorBasicInfo",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "createdTime",
                "type": "uint256",
                "internalType": "uint256"
            },
            {
                "name": "jailed",
                "type": "bool",
                "internalType": "bool"
            },
            {
                "name": "jailUntil",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorCommission",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "tuple",
                "internalType": "struct StakeHub.Commission",
                "components": [
                    {
                        "name": "rate",
                        "type": "uint64",
                        "internalType": "uint64"
                    },
                    {
                        "name": "maxRate",
                        "type": "uint64",
                        "internalType": "uint64"
                    },
                    {
                        "name": "maxChangeRate",
                        "type": "uint64",
                        "internalType": "uint64"
                    }
                ]
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorConsensusAddress",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorCreditContract",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "creditContract",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorDescription",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "tuple",
                "internalType": "struct StakeHub.Description",
                "components": [
                    {
                        "name": "moniker",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "identity",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "website",
                        "type": "string",
                        "internalType": "string"
                    },
                    {
                        "name": "details",
                        "type": "string",
                        "internalType": "string"
                    }
                ]
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorElectionInfo",
        "inputs": [
            {
                "name": "offset",
                "type": "uint256",
                "internalType": "uint256"
            },
            {
                "name": "limit",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [
            {
                "name": "consensusAddrs",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "votingPowers",
                "type": "uint256[]",
                "internalType": "uint256[]"
            },
            {
                "name": "voteAddrs",
                "type": "bytes[]",
                "internalType": "bytes[]"
            },
            {
                "name": "totalLength",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorRewardRecord",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "index",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorTotalPooledBNBRecord",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "index",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorUpdateTime",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidatorVoteAddress",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [
            {
                "name": "voteAddress",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getValidators",
        "inputs": [
            {
                "name": "offset",
                "type": "uint256",
                "internalType": "uint256"
            },
            {
                "name": "limit",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [
            {
                "name": "operatorAddrs",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "creditAddrs",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "totalLength",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "handleAckPackage",
        "inputs": [
            {
                "name": "channelId",
                "type": "uint8",
                "internalType": "uint8"
            },
            {
                "name": "msgBytes",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "handleFailAckPackage",
        "inputs": [
            {
                "name": "channelId",
                "type": "uint8",
                "internalType": "uint8"
            },
            {
                "name": "msgBytes",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "handleSynPackage",
        "inputs": [
            {
                "name": "",
                "type": "uint8",
                "internalType": "uint8"
            },
            {
                "name": "msgBytes",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "initialize",
        "inputs": [],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "isPaused",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "bool",
                "internalType": "bool"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "getNodeIDs",
        "inputs": [
            {
                "name": "validatorsToQuery",
                "type": "address[]",
                "internalType": "address[]"
            }
        ],
        "outputs": [
            {
                "name": "consensusAddresses",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "nodeIDsList",
                "type": "bytes32[][]",
                "internalType": "bytes32[][]"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "maliciousVoteSlash",
        "inputs": [
            {
                "name": "voteAddress",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "maxElectedValidators",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "maxFelonyBetweenBreatheBlock",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "maxNodeIDs",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "minDelegationBNBChange",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "minSelfDelegationBNB",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "numOfJailed",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "pause",
        "inputs": [],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "redelegate",
        "inputs": [
            {
                "name": "srcValidator",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "dstValidator",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "shares",
                "type": "uint256",
                "internalType": "uint256"
            },
            {
                "name": "delegateVotePower",
                "type": "bool",
                "internalType": "bool"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "redelegateFeeRate",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "removeFromBlackList",
        "inputs": [
            {
                "name": "account",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "removeNodeIDs",
        "inputs": [
            {
                "name": "targetNodeIDs",
                "type": "bytes32[]",
                "internalType": "bytes32[]"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "resume",
        "inputs": [],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "syncGovToken",
        "inputs": [
            {
                "name": "operatorAddresses",
                "type": "address[]",
                "internalType": "address[]"
            },
            {
                "name": "account",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "transferGasLimit",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "unbondPeriod",
        "inputs": [],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "undelegate",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            },
            {
                "name": "shares",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "unjail",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "updateAgent",
        "inputs": [
            {
                "name": "newAgent",
                "type": "address",
                "internalType": "address"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "updateParam",
        "inputs": [
            {
                "name": "key",
                "type": "string",
                "internalType": "string"
            },
            {
                "name": "value",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [],
        "stateMutability": "nonpayable"
    },
    {
        "type": "function",
        "name": "voteExpiration",
        "inputs": [
            {
                "name": "",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "uint256",
                "internalType": "uint256"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "function",
        "name": "voteToOperator",
        "inputs": [
            {
                "name": "",
                "type": "bytes",
                "internalType": "bytes"
            }
        ],
        "outputs": [
            {
                "name": "",
                "type": "address",
                "internalType": "address"
            }
        ],
        "stateMutability": "view"
    },
    {
        "type": "event",
        "name": "AgentChanged",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "oldAgent",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "newAgent",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "BlackListed",
        "inputs": [
            {
                "name": "target",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Claimed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "CommissionRateEdited",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "newCommissionRate",
                "type": "uint64",
                "indexed": false,
                "internalType": "uint64"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ConsensusAddressEdited",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "newConsensusAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Delegated",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "shares",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "DescriptionEdited",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Initialized",
        "inputs": [
            {
                "name": "version",
                "type": "uint8",
                "indexed": false,
                "internalType": "uint8"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "MigrateFailed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "respCode",
                "type": "uint8",
                "indexed": false,
                "internalType": "enum StakeHub.StakeMigrationRespCode"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "MigrateSuccess",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "shares",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "NodeIDAdded",
        "inputs": [
            {
                "name": "validator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "nodeID",
                "type": "bytes32",
                "indexed": false,
                "internalType": "bytes32"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "NodeIDRemoved",
        "inputs": [
            {
                "name": "validator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "nodeID",
                "type": "bytes32",
                "indexed": false,
                "internalType": "bytes32"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ParamChange",
        "inputs": [
            {
                "name": "key",
                "type": "string",
                "indexed": false,
                "internalType": "string"
            },
            {
                "name": "value",
                "type": "bytes",
                "indexed": false,
                "internalType": "bytes"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Paused",
        "inputs": [],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ProtectorChanged",
        "inputs": [
            {
                "name": "oldProtector",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "newProtector",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Redelegated",
        "inputs": [
            {
                "name": "srcValidator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "dstValidator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "oldShares",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "newShares",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Resumed",
        "inputs": [],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "RewardDistributeFailed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "failReason",
                "type": "bytes",
                "indexed": false,
                "internalType": "bytes"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "RewardDistributed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "reward",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "StakeCreditInitialized",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "creditContract",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "UnBlackListed",
        "inputs": [
            {
                "name": "target",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "Undelegated",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "delegator",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "shares",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "bnbAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "UnexpectedPackage",
        "inputs": [
            {
                "name": "channelId",
                "type": "uint8",
                "indexed": false,
                "internalType": "uint8"
            },
            {
                "name": "msgBytes",
                "type": "bytes",
                "indexed": false,
                "internalType": "bytes"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ValidatorCreated",
        "inputs": [
            {
                "name": "consensusAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "creditContract",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "voteAddress",
                "type": "bytes",
                "indexed": false,
                "internalType": "bytes"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ValidatorEmptyJailed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ValidatorJailed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ValidatorSlashed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "jailUntil",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "slashAmount",
                "type": "uint256",
                "indexed": false,
                "internalType": "uint256"
            },
            {
                "name": "slashType",
                "type": "uint8",
                "indexed": false,
                "internalType": "enum StakeHub.SlashType"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "ValidatorUnjailed",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            }
        ],
        "anonymous": false
    },
    {
        "type": "event",
        "name": "VoteAddressEdited",
        "inputs": [
            {
                "name": "operatorAddress",
                "type": "address",
                "indexed": true,
                "internalType": "address"
            },
            {
                "name": "newVoteAddress",
                "type": "bytes",
                "indexed": false,
                "internalType": "bytes"
            }
        ],
        "anonymous": false
    },
    {
        "type": "error",
        "name": "AlreadyPaused",
        "inputs": []
    },
    {
        "type": "error",
        "name": "AlreadySlashed",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ConsensusAddressExpired",
        "inputs": []
    },
    {
        "type": "error",
        "name": "DelegationAmountTooSmall",
        "inputs": []
    },
    {
        "type": "error",
        "name": "DuplicateConsensusAddress",
        "inputs": []
    },
    {
        "type": "error",
        "name": "DuplicateMoniker",
        "inputs": []
    },
    {
        "type": "error",
        "name": "DuplicateNodeID",
        "inputs": []
    },
    {
        "type": "error",
        "name": "DuplicateVoteAddress",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ExceedsMaxNodeIDs",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InBlackList",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidAgent",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidCommission",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidConsensusAddress",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidMoniker",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidNodeID",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidRequest",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidSynPackage",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidValidator",
        "inputs": []
    },
    {
        "type": "error",
        "name": "InvalidValue",
        "inputs": [
            {
                "name": "key",
                "type": "string",
                "internalType": "string"
            },
            {
                "name": "value",
                "type": "bytes",
                "internalType": "bytes"
            }
        ]
    },
    {
        "type": "error",
        "name": "InvalidVoteAddress",
        "inputs": []
    },
    {
        "type": "error",
        "name": "JailTimeNotExpired",
        "inputs": []
    },
    {
        "type": "error",
        "name": "NoMoreFelonyAllowed",
        "inputs": []
    },
    {
        "type": "error",
        "name": "NotPaused",
        "inputs": []
    },
    {
        "type": "error",
        "name": "OnlyCoinbase",
        "inputs": []
    },
    {
        "type": "error",
        "name": "OnlyProtector",
        "inputs": []
    },
    {
        "type": "error",
        "name": "OnlySelfDelegation",
        "inputs": []
    },
    {
        "type": "error",
        "name": "OnlySystemContract",
        "inputs": [
            {
                "name": "systemContract",
                "type": "address",
                "internalType": "address"
            }
        ]
    },
    {
        "type": "error",
        "name": "OnlyZeroGasPrice",
        "inputs": []
    },
    {
        "type": "error",
        "name": "SameValidator",
        "inputs": []
    },
    {
        "type": "error",
        "name": "SelfDelegationNotEnough",
        "inputs": []
    },
    {
        "type": "error",
        "name": "TransferFailed",
        "inputs": []
    },
    {
        "type": "error",
        "name": "UnknownParam",
        "inputs": [
            {
                "name": "key",
                "type": "string",
                "internalType": "string"
            },
            {
                "name": "value",
                "type": "bytes",
                "internalType": "bytes"
            }
        ]
    },
    {
        "type": "error",
        "name": "UpdateTooFrequently",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ValidatorExisted",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ValidatorNotExisted",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ValidatorNotJailed",
        "inputs": []
    },
    {
        "type": "error",
        "name": "VoteAddressExpired",
        "inputs": []
    },
    {
        "type": "error",
        "name": "ZeroShares",
        "inputs": []
    }
]"#;
}

#[cfg(test)]
mod tests {

    use alloy_dyn_abi::{DynSolValue, FunctionExt, JsonAbiExt};
    use alloy_json_abi::JsonAbi;
    use alloy_primitives::{address, hex, Address, U256};

    use super::*;

    #[test]
    fn abi_encode() {
        let expected = "63a036b500000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000";

        let stake_hub_abi: JsonAbi =
            serde_json::from_str(*STAKE_HUB_ABI).unwrap();
        let function = stake_hub_abi
            .function("getValidatorElectionInfo")
            .unwrap()
            .first()
            .unwrap();
        let input = function
            .abi_encode_input(&[
                DynSolValue::from(U256::from(0)),
                DynSolValue::from(U256::from(0)),
            ])
            .unwrap();

        let input_str = hex::encode(&input);
        println!("encoded data: {:?}", &input_str);
        assert_eq!(input_str, expected);
    }

    #[test]
    fn abi_decode() {
        let expected_consensus_addr =
            address!("C08B5542D177ac6686946920409741463a15dDdB");
        let expected_voting_power = U256::from(1);
        let expected_vote_addr = hex::decode("3c2438a4113804bf99e3849ef31887c0f880a0feb92f356f58fbd023a82f5311fc87a5883a662e9ebbbefc90bf13aa53").unwrap();
        let expected_total_length = U256::from(1);

        let output_str = "000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000c0000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000c08b5542d177ac6686946920409741463a15dddb000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000303c2438a4113804bf99e3849ef31887c0f880a0feb92f356f58fbd023a82f5311fc87a5883a662e9ebbbefc90bf13aa5300000000000000000000000000000000";
        let output = hex::decode(output_str).unwrap();

        let stake_hub_abi: JsonAbi =
            serde_json::from_str(*STAKE_HUB_ABI).unwrap();
        let function = stake_hub_abi
            .function("getValidatorElectionInfo")
            .unwrap()
            .first()
            .unwrap();
        let output = function.abi_decode_output(&output).unwrap();

        let consensus_address: Vec<Address> = output[0]
            .as_array()
            .unwrap()
            .iter()
            .map(|val| val.as_address().unwrap())
            .collect();
        let voting_powers: Vec<U256> = output[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|val| val.as_uint().unwrap().0)
            .collect();
        let vote_addresses: Vec<Vec<u8>> = output[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|val| val.as_bytes().unwrap().to_vec())
            .collect();
        let total_length = output[3].as_uint().unwrap().0;

        println!("consensus address: {:?}", consensus_address[0]);
        println!("voting power: {:?}", voting_powers[0]);
        println!("vote address: {:?}", hex::encode(&vote_addresses[0]));
        println!("total length: {total_length:?}");

        assert_eq!(consensus_address[0], expected_consensus_addr);
        assert_eq!(voting_powers[0], expected_voting_power);
        assert_eq!(vote_addresses[0], expected_vote_addr);
        assert_eq!(total_length, expected_total_length);
    }
}
