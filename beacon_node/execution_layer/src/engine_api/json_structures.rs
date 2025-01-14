use super::*;
use serde::{Deserialize, Serialize};
use superstruct::superstruct;
use types::blobs_sidecar::KzgCommitments;
use types::{
    Blobs, EthSpec, ExecutionBlockHash, ExecutionPayload, ExecutionPayloadCapella,
    ExecutionPayloadEip4844, ExecutionPayloadMerge, FixedVector, Transaction, Unsigned,
    VariableList, Withdrawal,
};

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonRequestBody<'a> {
    pub jsonrpc: &'a str,
    pub method: &'a str,
    pub params: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonResponseBody {
    pub jsonrpc: String,
    #[serde(default)]
    pub error: Option<JsonError>,
    #[serde(default)]
    pub result: serde_json::Value,
    pub id: serde_json::Value,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransparentJsonPayloadId(#[serde(with = "eth2_serde_utils::bytes_8_hex")] pub PayloadId);

impl From<PayloadId> for TransparentJsonPayloadId {
    fn from(id: PayloadId) -> Self {
        Self(id)
    }
}

impl From<TransparentJsonPayloadId> for PayloadId {
    fn from(wrapper: TransparentJsonPayloadId) -> Self {
        wrapper.0
    }
}

/// On the request, use a transparent wrapper.
pub type JsonPayloadIdRequest = TransparentJsonPayloadId;

/// On the response, expect without the object wrapper (non-transparent).
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonPayloadIdResponse {
    #[serde(with = "eth2_serde_utils::bytes_8_hex")]
    pub payload_id: PayloadId,
}

#[superstruct(
    variants(V1, V2),
    variant_attributes(
        derive(Debug, PartialEq, Default, Serialize, Deserialize,),
        serde(bound = "T: EthSpec", rename_all = "camelCase"),
    ),
    cast_error(ty = "Error", expr = "Error::IncorrectStateVariant"),
    partial_getter_error(ty = "Error", expr = "Error::IncorrectStateVariant")
)]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound = "T: EthSpec", rename_all = "camelCase", untagged)]
pub struct JsonExecutionPayload<T: EthSpec> {
    pub parent_hash: ExecutionBlockHash,
    pub fee_recipient: Address,
    pub state_root: Hash256,
    pub receipts_root: Hash256,
    #[serde(with = "serde_logs_bloom")]
    pub logs_bloom: FixedVector<u8, T::BytesPerLogsBloom>,
    pub prev_randao: Hash256,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub block_number: u64,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub gas_limit: u64,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub gas_used: u64,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub timestamp: u64,
    #[serde(with = "ssz_types::serde_utils::hex_var_list")]
    pub extra_data: VariableList<u8, T::MaxExtraDataBytes>,
    #[serde(with = "eth2_serde_utils::u256_hex_be")]
    pub base_fee_per_gas: Uint256,
    #[superstruct(only(V2))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    // #[serde(with = "eth2_serde_utils::u256_hex_be_opt")]
    #[serde(with = "remove_this")]
    pub excess_data_gas: Option<Uint256>,
    pub block_hash: ExecutionBlockHash,
    #[serde(with = "ssz_types::serde_utils::list_of_hex_var_list")]
    pub transactions:
        VariableList<Transaction<T::MaxBytesPerTransaction>, T::MaxTransactionsPerPayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    #[superstruct(only(V2))]
    pub withdrawals: Option<VariableList<JsonWithdrawal, T::MaxWithdrawalsPerPayload>>,
}

pub mod remove_this {
    use super::Uint256 as U256;

    use serde::de::Visitor;
    use serde::{de, Deserializer, Serialize, Serializer};
    use std::fmt;
    use std::str::FromStr;

    pub fn serialize<S>(num: &Option<U256>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        num.serialize(serializer)
    }

    pub struct U256Visitor;

    impl<'de> Visitor<'de> for U256Visitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a well formatted hex string")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if !value.starts_with("0x") {
                return Err(de::Error::custom("must start with 0x"));
            }
            let stripped = &value[2..];
            if stripped.is_empty() {
                Err(de::Error::custom(format!(
                    "quantity cannot be {:?}",
                    stripped
                )))
            } else if stripped == "0" {
                Ok(value.to_string())
            } else if stripped.starts_with('0') {
                Err(de::Error::custom("cannot have leading zero"))
            } else {
                Ok(value.to_string())
            }
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<U256>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let decoded = deserializer.deserialize_string(U256Visitor)?;

        Some(
            U256::from_str(&decoded)
                .map_err(|e| de::Error::custom(format!("Invalid U256 string: {}", e))),
        )
        .transpose()
    }
}

impl<T: EthSpec> JsonExecutionPayload<T> {
    pub fn try_into_execution_payload(
        self,
        fork_name: ForkName,
    ) -> Result<ExecutionPayload<T>, Error> {
        match self {
            JsonExecutionPayload::V1(v1) => match fork_name {
                ForkName::Merge => Ok(ExecutionPayload::Merge(ExecutionPayloadMerge {
                    parent_hash: v1.parent_hash,
                    fee_recipient: v1.fee_recipient,
                    state_root: v1.state_root,
                    receipts_root: v1.receipts_root,
                    logs_bloom: v1.logs_bloom,
                    prev_randao: v1.prev_randao,
                    block_number: v1.block_number,
                    gas_limit: v1.gas_limit,
                    gas_used: v1.gas_used,
                    timestamp: v1.timestamp,
                    extra_data: v1.extra_data,
                    base_fee_per_gas: v1.base_fee_per_gas,
                    block_hash: v1.block_hash,
                    transactions: v1.transactions,
                })),
                _ => Err(Error::UnsupportedForkVariant(format!("Unsupported conversion from JsonExecutionPayloadV1 for {}", fork_name))),
            }
            JsonExecutionPayload::V2(v2) => match fork_name {
                ForkName::Merge => Ok(ExecutionPayload::Merge(ExecutionPayloadMerge {
                    parent_hash: v2.parent_hash,
                    fee_recipient: v2.fee_recipient,
                    state_root: v2.state_root,
                    receipts_root: v2.receipts_root,
                    logs_bloom: v2.logs_bloom,
                    prev_randao: v2.prev_randao,
                    block_number: v2.block_number,
                    gas_limit: v2.gas_limit,
                    gas_used: v2.gas_used,
                    timestamp: v2.timestamp,
                    extra_data: v2.extra_data,
                    base_fee_per_gas: v2.base_fee_per_gas,
                    block_hash: v2.block_hash,
                    transactions: v2.transactions,
                })),
                ForkName::Capella => Ok(ExecutionPayload::Capella(ExecutionPayloadCapella {
                    parent_hash: v2.parent_hash,
                    fee_recipient: v2.fee_recipient,
                    state_root: v2.state_root,
                    receipts_root: v2.receipts_root,
                    logs_bloom: v2.logs_bloom,
                    prev_randao: v2.prev_randao,
                    block_number: v2.block_number,
                    gas_limit: v2.gas_limit,
                    gas_used: v2.gas_used,
                    timestamp: v2.timestamp,
                    extra_data: v2.extra_data,
                    base_fee_per_gas: v2.base_fee_per_gas,
                    block_hash: v2.block_hash,
                    transactions: v2.transactions,
                    withdrawals: v2
                        .withdrawals
                        .map(|v| {
                            Into::<Vec<_>>::into(v)
                                .into_iter()
                                .map(Into::into)
                                .collect::<Vec<_>>()
                                .into()
                        })
                        .ok_or_else(|| Error::BadConversion("Null withdrawal field converting JsonExecutionPayloadV2 -> ExecutionPayloadCapella".to_string()))?
                })),
                ForkName::Eip4844 => Ok(ExecutionPayload::Eip4844(ExecutionPayloadEip4844 {
                    parent_hash: v2.parent_hash,
                    fee_recipient: v2.fee_recipient,
                    state_root: v2.state_root,
                    receipts_root: v2.receipts_root,
                    logs_bloom: v2.logs_bloom,
                    prev_randao: v2.prev_randao,
                    block_number: v2.block_number,
                    gas_limit: v2.gas_limit,
                    gas_used: v2.gas_used,
                    timestamp: v2.timestamp,
                    extra_data: v2.extra_data,
                    base_fee_per_gas: v2.base_fee_per_gas,
                    excess_data_gas: v2.excess_data_gas.ok_or_else(|| Error::BadConversion("Null `excess_data_gas` field converting JsonExecutionPayloadV2 -> ExecutionPayloadEip4844".to_string()))?,
                    block_hash: v2.block_hash,
                    transactions: v2.transactions,
                    withdrawals: v2
                        .withdrawals
                        .map(|v| {
                            Into::<Vec<_>>::into(v)
                                .into_iter()
                                .map(Into::into)
                                .collect::<Vec<_>>()
                                .into()
                        })
                        .ok_or_else(|| Error::BadConversion("Null withdrawal field converting JsonExecutionPayloadV2 -> ExecutionPayloadEip4844".to_string()))?
                })),
                _ => Err(Error::UnsupportedForkVariant(format!("Unsupported conversion from JsonExecutionPayloadV2 for {}", fork_name))),
            }
        }
    }
}

impl<T: EthSpec> TryFrom<ExecutionPayload<T>> for JsonExecutionPayloadV1<T> {
    type Error = Error;
    fn try_from(payload: ExecutionPayload<T>) -> Result<Self, Error> {
        match payload {
            ExecutionPayload::Merge(merge) => Ok(JsonExecutionPayloadV1 {
                parent_hash: merge.parent_hash,
                fee_recipient: merge.fee_recipient,
                state_root: merge.state_root,
                receipts_root: merge.receipts_root,
                logs_bloom: merge.logs_bloom,
                prev_randao: merge.prev_randao,
                block_number: merge.block_number,
                gas_limit: merge.gas_limit,
                gas_used: merge.gas_used,
                timestamp: merge.timestamp,
                extra_data: merge.extra_data,
                base_fee_per_gas: merge.base_fee_per_gas,
                block_hash: merge.block_hash,
                transactions: merge.transactions,
            }),
            ExecutionPayload::Capella(_) => Err(Error::UnsupportedForkVariant(format!(
                "Unsupported conversion to JsonExecutionPayloadV1 for {}",
                ForkName::Capella
            ))),
            ExecutionPayload::Eip4844(_) => Err(Error::UnsupportedForkVariant(format!(
                "Unsupported conversion to JsonExecutionPayloadV1 for {}",
                ForkName::Eip4844
            ))),
        }
    }
}

impl<T: EthSpec> TryFrom<ExecutionPayload<T>> for JsonExecutionPayloadV2<T> {
    type Error = Error;
    fn try_from(payload: ExecutionPayload<T>) -> Result<Self, Error> {
        match payload {
            ExecutionPayload::Merge(merge) => Ok(JsonExecutionPayloadV2 {
                parent_hash: merge.parent_hash,
                fee_recipient: merge.fee_recipient,
                state_root: merge.state_root,
                receipts_root: merge.receipts_root,
                logs_bloom: merge.logs_bloom,
                prev_randao: merge.prev_randao,
                block_number: merge.block_number,
                gas_limit: merge.gas_limit,
                gas_used: merge.gas_used,
                timestamp: merge.timestamp,
                extra_data: merge.extra_data,
                base_fee_per_gas: merge.base_fee_per_gas,
                excess_data_gas: None,
                block_hash: merge.block_hash,
                transactions: merge.transactions,
                withdrawals: None,
            }),
            ExecutionPayload::Capella(capella) => Ok(JsonExecutionPayloadV2 {
                parent_hash: capella.parent_hash,
                fee_recipient: capella.fee_recipient,
                state_root: capella.state_root,
                receipts_root: capella.receipts_root,
                logs_bloom: capella.logs_bloom,
                prev_randao: capella.prev_randao,
                block_number: capella.block_number,
                gas_limit: capella.gas_limit,
                gas_used: capella.gas_used,
                timestamp: capella.timestamp,
                extra_data: capella.extra_data,
                base_fee_per_gas: capella.base_fee_per_gas,
                excess_data_gas: None,
                block_hash: capella.block_hash,
                transactions: capella.transactions,
                withdrawals: Some(
                    Vec::from(capella.withdrawals)
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>()
                        .into(),
                ),
            }),
            ExecutionPayload::Eip4844(eip4844) => Ok(JsonExecutionPayloadV2 {
                parent_hash: eip4844.parent_hash,
                fee_recipient: eip4844.fee_recipient,
                state_root: eip4844.state_root,
                receipts_root: eip4844.receipts_root,
                logs_bloom: eip4844.logs_bloom,
                prev_randao: eip4844.prev_randao,
                block_number: eip4844.block_number,
                gas_limit: eip4844.gas_limit,
                gas_used: eip4844.gas_used,
                timestamp: eip4844.timestamp,
                extra_data: eip4844.extra_data,
                base_fee_per_gas: eip4844.base_fee_per_gas,
                excess_data_gas: Some(eip4844.excess_data_gas),
                block_hash: eip4844.block_hash,
                transactions: eip4844.transactions,
                withdrawals: Some(
                    Vec::from(eip4844.withdrawals)
                        .into_iter()
                        .map(Into::into)
                        .collect::<Vec<_>>()
                        .into(),
                ),
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonWithdrawal {
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub index: u64,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub validator_index: u64,
    pub address: Address,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub amount: u64,
}

impl From<Withdrawal> for JsonWithdrawal {
    fn from(withdrawal: Withdrawal) -> Self {
        Self {
            index: withdrawal.index,
            validator_index: withdrawal.validator_index,
            address: withdrawal.address,
            amount: withdrawal.amount,
        }
    }
}

impl From<JsonWithdrawal> for Withdrawal {
    fn from(jw: JsonWithdrawal) -> Self {
        Self {
            index: jw.index,
            validator_index: jw.validator_index,
            address: jw.address,
            amount: jw.amount,
        }
    }
}

#[superstruct(
    variants(V1, V2),
    variant_attributes(
        derive(Clone, Debug, PartialEq, Serialize, Deserialize),
        serde(rename_all = "camelCase")
    ),
    cast_error(ty = "Error", expr = "Error::IncorrectStateVariant"),
    partial_getter_error(ty = "Error", expr = "Error::IncorrectStateVariant")
)]
#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub struct JsonPayloadAttributes {
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub timestamp: u64,
    pub prev_randao: Hash256,
    pub suggested_fee_recipient: Address,
    #[superstruct(only(V2))]
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    pub withdrawals: Option<Vec<JsonWithdrawal>>,
}

impl From<PayloadAttributes> for JsonPayloadAttributes {
    fn from(payload_atributes: PayloadAttributes) -> Self {
        match payload_atributes {
            PayloadAttributes::V1(pa) => Self::V1(JsonPayloadAttributesV1 {
                timestamp: pa.timestamp,
                prev_randao: pa.prev_randao,
                suggested_fee_recipient: pa.suggested_fee_recipient,
            }),
            PayloadAttributes::V2(pa) => Self::V2(JsonPayloadAttributesV2 {
                timestamp: pa.timestamp,
                prev_randao: pa.prev_randao,
                suggested_fee_recipient: pa.suggested_fee_recipient,
                withdrawals: pa
                    .withdrawals
                    .map(|w| w.into_iter().map(Into::into).collect()),
            }),
        }
    }
}

impl From<JsonPayloadAttributes> for PayloadAttributes {
    fn from(json_payload_attributes: JsonPayloadAttributes) -> Self {
        match json_payload_attributes {
            JsonPayloadAttributes::V1(jpa) => Self::V1(PayloadAttributesV1 {
                timestamp: jpa.timestamp,
                prev_randao: jpa.prev_randao,
                suggested_fee_recipient: jpa.suggested_fee_recipient,
            }),
            JsonPayloadAttributes::V2(jpa) => Self::V2(PayloadAttributesV2 {
                timestamp: jpa.timestamp,
                prev_randao: jpa.prev_randao,
                suggested_fee_recipient: jpa.suggested_fee_recipient,
                withdrawals: jpa
                    .withdrawals
                    .map(|jw| jw.into_iter().map(Into::into).collect()),
            }),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(bound = "T: EthSpec", rename_all = "camelCase")]
pub struct JsonBlobsBundle<T: EthSpec> {
    pub block_hash: ExecutionBlockHash,
    pub kzgs: KzgCommitments<T>,
    #[serde(with = "ssz_types::serde_utils::list_of_hex_fixed_vec")]
    pub blobs: Blobs<T>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonForkchoiceStateV1 {
    pub head_block_hash: ExecutionBlockHash,
    pub safe_block_hash: ExecutionBlockHash,
    pub finalized_block_hash: ExecutionBlockHash,
}

impl From<ForkchoiceState> for JsonForkchoiceStateV1 {
    fn from(f: ForkchoiceState) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let ForkchoiceState {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        } = f;

        Self {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        }
    }
}

impl From<JsonForkchoiceStateV1> for ForkchoiceState {
    fn from(j: JsonForkchoiceStateV1) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let JsonForkchoiceStateV1 {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        } = j;

        Self {
            head_block_hash,
            safe_block_hash,
            finalized_block_hash,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum JsonPayloadStatusV1Status {
    Valid,
    Invalid,
    Syncing,
    Accepted,
    InvalidBlockHash,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonPayloadStatusV1 {
    pub status: JsonPayloadStatusV1Status,
    pub latest_valid_hash: Option<ExecutionBlockHash>,
    pub validation_error: Option<String>,
}

impl From<PayloadStatusV1Status> for JsonPayloadStatusV1Status {
    fn from(e: PayloadStatusV1Status) -> Self {
        match e {
            PayloadStatusV1Status::Valid => JsonPayloadStatusV1Status::Valid,
            PayloadStatusV1Status::Invalid => JsonPayloadStatusV1Status::Invalid,
            PayloadStatusV1Status::Syncing => JsonPayloadStatusV1Status::Syncing,
            PayloadStatusV1Status::Accepted => JsonPayloadStatusV1Status::Accepted,
            PayloadStatusV1Status::InvalidBlockHash => JsonPayloadStatusV1Status::InvalidBlockHash,
        }
    }
}
impl From<JsonPayloadStatusV1Status> for PayloadStatusV1Status {
    fn from(j: JsonPayloadStatusV1Status) -> Self {
        match j {
            JsonPayloadStatusV1Status::Valid => PayloadStatusV1Status::Valid,
            JsonPayloadStatusV1Status::Invalid => PayloadStatusV1Status::Invalid,
            JsonPayloadStatusV1Status::Syncing => PayloadStatusV1Status::Syncing,
            JsonPayloadStatusV1Status::Accepted => PayloadStatusV1Status::Accepted,
            JsonPayloadStatusV1Status::InvalidBlockHash => PayloadStatusV1Status::InvalidBlockHash,
        }
    }
}

impl From<PayloadStatusV1> for JsonPayloadStatusV1 {
    fn from(p: PayloadStatusV1) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let PayloadStatusV1 {
            status,
            latest_valid_hash,
            validation_error,
        } = p;

        Self {
            status: status.into(),
            latest_valid_hash,
            validation_error,
        }
    }
}

impl From<JsonPayloadStatusV1> for PayloadStatusV1 {
    fn from(j: JsonPayloadStatusV1) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let JsonPayloadStatusV1 {
            status,
            latest_valid_hash,
            validation_error,
        } = j;

        Self {
            status: status.into(),
            latest_valid_hash,
            validation_error,
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JsonForkchoiceUpdatedV1Response {
    pub payload_status: JsonPayloadStatusV1,
    pub payload_id: Option<TransparentJsonPayloadId>,
}

impl From<JsonForkchoiceUpdatedV1Response> for ForkchoiceUpdatedResponse {
    fn from(j: JsonForkchoiceUpdatedV1Response) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let JsonForkchoiceUpdatedV1Response {
            payload_status: status,
            payload_id,
        } = j;

        Self {
            payload_status: status.into(),
            payload_id: payload_id.map(Into::into),
        }
    }
}
impl From<ForkchoiceUpdatedResponse> for JsonForkchoiceUpdatedV1Response {
    fn from(f: ForkchoiceUpdatedResponse) -> Self {
        // Use this verbose deconstruction pattern to ensure no field is left unused.
        let ForkchoiceUpdatedResponse {
            payload_status: status,
            payload_id,
        } = f;

        Self {
            payload_status: status.into(),
            payload_id: payload_id.map(Into::into),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionConfigurationV1 {
    #[serde(with = "eth2_serde_utils::u256_hex_be")]
    pub terminal_total_difficulty: Uint256,
    pub terminal_block_hash: ExecutionBlockHash,
    #[serde(with = "eth2_serde_utils::u64_hex_be")]
    pub terminal_block_number: u64,
}

/// Serializes the `logs_bloom` field of an `ExecutionPayload`.
pub mod serde_logs_bloom {
    use super::*;
    use eth2_serde_utils::hex::PrefixedHexVisitor;
    use serde::{Deserializer, Serializer};

    pub fn serialize<S, U>(bytes: &FixedVector<u8, U>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        U: Unsigned,
    {
        let mut hex_string: String = "0x".to_string();
        hex_string.push_str(&hex::encode(&bytes[..]));

        serializer.serialize_str(&hex_string)
    }

    pub fn deserialize<'de, D, U>(deserializer: D) -> Result<FixedVector<u8, U>, D::Error>
    where
        D: Deserializer<'de>,
        U: Unsigned,
    {
        let vec = deserializer.deserialize_string(PrefixedHexVisitor)?;

        FixedVector::new(vec)
            .map_err(|e| serde::de::Error::custom(format!("invalid logs bloom: {:?}", e)))
    }
}
