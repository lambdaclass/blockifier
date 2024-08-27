use std::{collections::HashMap, fs, path::Path, sync::Arc};

use crate::{
    abi::abi_utils::selector_from_name,
    context::{BlockContext, TransactionContext},
    declare_tx_args,
    execution::{call_info::CallInfo, common_hints::ExecutionMode, contract_class::{ContractClass, ContractClassV1, NativeContractClassV1}, entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext}, execution_utils::execute_entry_point_call},
    state::{cached_state::CachedState, state_api::StateReader},
    test_utils::{
        declare::declare_tx, deploy_contract, dict_state_reader::DictStateReader, MAX_FEE,
        MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE,
    },
    transaction::{
        objects::TransactionInfo,
        test_utils::{calculate_class_info_for_testing, l1_resource_bounds},
        transactions::ExecutableTransaction,
    },
};
use starknet_api::{
    class_hash, core::{ClassHash, ContractAddress, Nonce}, deprecated_contract_class::EntryPointType, felt, hash::StarkHash, transaction::{Calldata, Fee, TransactionVersion}
};
use starknet_types_core::felt::Felt;

pub const ACCOUNT_ADDRESS: u32 = 4321;
pub const OWNER_ADDRESS: u32 = 4321;

const BENCH_YAS: &str = "bench/yas/";
const CLASS_HASH_BASE: u32 = 1 << 16;
const YAS_CUSTOM_ACCOUNT_BASE: u32 = CLASS_HASH_BASE;
const YAS_FACTORY_BASE: u32 = 2 * CLASS_HASH_BASE;
const YAS_POOL_BASE: u32 = 3 * CLASS_HASH_BASE;
const YAS_ROUTER_BASE: u32 = 4 * CLASS_HASH_BASE;
const YAS_ERC_BASE: u32 = 5 * CLASS_HASH_BASE;

fn declare_contract<S: StateReader>(
    state: &mut CachedState<S>,
    contract_name: &str,
    cairo_native: bool,
) -> Result<ClassHash, Box<dyn std::error::Error>> {
    let contract_class = load_contract(contract_name, cairo_native)?;
    let block_context = &BlockContext::create_for_testing();
    let class_info = calculate_class_info_for_testing(contract_class);
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let class_hash = get_class_hash(contract_name);
    let nonce = state.get_nonce_at(sender_address)?;
    let declare_args = declare_tx_args! {
        max_fee: Fee(MAX_FEE),
        sender_address,
        version: TransactionVersion::THREE,
        resource_bounds: l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE),
        class_hash,
        nonce
    };

    declare_tx(declare_args, class_info.clone()).execute(state, block_context, false, true)?;

    let contract_class_from_state = state.get_compiled_contract_class(class_hash).unwrap();
    assert_eq!(contract_class_from_state, class_info.contract_class());

    Ok(class_hash)
}

pub fn declare_all_contracts(
    state: &mut CachedState<DictStateReader>,
    cairo_native: bool,
) -> Result<(ClassHash, ClassHash, ClassHash, ClassHash), Box<dyn std::error::Error>> {
    let erc20_class_hash = declare_contract(state, "ERC20", cairo_native)?;
    let yas_factory_class_hash = declare_contract(state, "YASFactory", cairo_native)?;
    let yas_router_class_hash = declare_contract(state, "YASRouter", cairo_native)?;
    let yas_pool_class_hash = declare_contract(state, "YASPool", cairo_native)?;

    Ok((erc20_class_hash, yas_factory_class_hash, yas_router_class_hash, yas_pool_class_hash))
}

fn tx_deploy_contract<S: StateReader>(
    state: &mut CachedState<S>,
    calldata: &[Felt],
    class_hash: Felt,
) -> Result<Felt, Box<dyn std::error::Error>> {
    let salt = state.get_nonce_at(ContractAddress(ACCOUNT_ADDRESS.into()))?;
    let salt = StarkHash::from(salt.0);
    let (address, _): (Felt, Vec<Felt>) =
        deploy_contract(state, class_hash, salt, calldata).unwrap();

    Ok(address)
}

pub fn deploy_all_contracts(
    state: &mut CachedState<DictStateReader>,
    erc20_class_hash: ClassHash,
    yas_factory_class_hash: ClassHash,
    yas_router_class_hash: ClassHash,
    yas_pool_class_hash: ClassHash,
) -> Result<(Felt, Felt, Felt, Felt), Box<dyn std::error::Error>> {
    let name = Felt::from_bytes_be_slice("TYAS0".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS0".as_bytes());

    let calldata = vec![
        name,
        symbol,
        0_u128.into(),
        0x9876_dace_9d90_0000_0000_u128.into(),
        OWNER_ADDRESS.into(),
    ];
    let yas0_token_address =
        tx_deploy_contract(state, &calldata, StarkHash::from(erc20_class_hash.0))?;

    let name = Felt::from_bytes_be_slice("TYAS1".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS1".as_bytes());

    let calldata = vec![
        name,
        symbol,
        0_u128.into(),
        0x9876_dace_9d90_0000_0000_u128.into(),
        OWNER_ADDRESS.into(),
    ];
    let yas1_token_address =
        tx_deploy_contract(state, &calldata, StarkHash::from(erc20_class_hash.0))?;

    let calldata = vec![OWNER_ADDRESS.into(), StarkHash::from(yas_pool_class_hash.0)];
    let yas_factory_address =
        tx_deploy_contract(state, &calldata, StarkHash::from(yas_factory_class_hash.0))?;

    let calldata = vec![];
    let yas_router_address =
        tx_deploy_contract(state, &calldata, StarkHash::from(yas_router_class_hash.0))?;

    let calldata = vec![
        yas_factory_address,
        yas0_token_address,
        yas1_token_address,
        0x0bb8.into(),
        0x3c.into(),
        0.into(),
    ];
    let yas_pool_address =
        tx_deploy_contract(state, &calldata, StarkHash::from(yas_pool_class_hash.0))?;

    Ok((yas0_token_address, yas1_token_address, yas_router_address, yas_pool_address))
}

pub fn invoke_func(
    state: &mut CachedState<DictStateReader>,
    entry_point: &str,
    contract_address: Felt,
    calldata: Calldata,
) -> Result<CallInfo, Box<dyn std::error::Error>> {
    let caller_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let contract_address = ContractAddress(StarkHash::from(contract_address).try_into()?);
    let mut context = EntryPointExecutionContext::new(
        Arc::new(TransactionContext {
            block_context: BlockContext::create_for_testing(),
            tx_info: TransactionInfo::Current(Default::default()),
        }),
        ExecutionMode::Execute,
        false,
    )
    .unwrap();

    let class_hash = state.get_class_hash_at(contract_address)?;
    let contract_class = state.get_compiled_contract_class(class_hash)?;
    let call = CallEntryPoint {
        class_hash: Some(class_hash),
        caller_address,
        code_address: Some(contract_address),
        entry_point_type: EntryPointType::External,
        entry_point_selector: selector_from_name(entry_point),
        calldata,
        storage_address: contract_address,
        call_type: CallType::Call,
        initial_gas: u64::MAX,
    };

    let call_info = execute_entry_point_call(
        call,
        contract_class,
        state,
        &mut Default::default(),
        &mut context,
    )
    .map_err(|e| e.to_string())
    .unwrap();

    Ok(call_info)
}

pub fn get_class_hash(contract: &str) -> ClassHash {
    class_hash!(integer_base(contract))
}

fn integer_base(contract: &str) -> u32 {
    let cairo1_bit = 1 << 31_i32;
    let base = match contract {
        "YasCustomAccount" => YAS_CUSTOM_ACCOUNT_BASE,
        "ERC20" => YAS_ERC_BASE,
        "YASFactory" => YAS_FACTORY_BASE,
        "YASPool" => YAS_POOL_BASE,
        "YASRouter" => YAS_ROUTER_BASE,
        name => panic!("Not valied contract name: {}", name),
    };

    base + cairo1_bit
}

pub fn get_balance<S: StateReader>(
    state: &mut CachedState<S>,
    token_address: Felt,
) -> Result<StarkHash, Box<dyn std::error::Error>> {
    let (low, high) = state.get_fee_token_balance(
        ContractAddress(OWNER_ADDRESS.into()),
        ContractAddress(StarkHash::from(token_address).try_into()?),
    )?;

    let low = &low.to_bytes_be()[15..];
    let high = &high.to_bytes_be()[15..];

    let balance = Felt::from_bytes_be_slice(&[high, low].concat()) ;

    Ok(balance)
}

pub fn create_state(
    cairo_native: bool,
) -> Result<CachedState<DictStateReader>, Box<dyn std::error::Error>> {
    let mut class_hash_to_class = HashMap::new();
    let mut address_to_class_hash = HashMap::new();
    let mut address_to_nonce = HashMap::new();

    let contract_class = load_contract("YasCustomAccount", cairo_native)?;
    let class_hash = get_class_hash("YasCustomAccount");

    address_to_class_hash.insert(ContractAddress(ACCOUNT_ADDRESS.into()), class_hash);
    class_hash_to_class.insert(class_hash, contract_class);
    address_to_nonce
        .insert(ContractAddress(ACCOUNT_ADDRESS.into()), Nonce(StarkHash::from(1)));

    let state_reader = DictStateReader {
        class_hash_to_class,
        address_to_class_hash,
        address_to_nonce,
        ..Default::default()
    };

    Ok(CachedState::new(state_reader))
}

pub fn load_contract(
    name: &str,
    cairo_native: bool,
) -> Result<ContractClass, Box<dyn std::error::Error>> {
    let path = Path::new(BENCH_YAS).join(name);

    if !cairo_native {
        let casm_json = &fs::read_to_string(path.with_extension("json"))?;
        Ok(ContractClass::V1(ContractClassV1::try_from_json_string(&casm_json)?))
    } else {
        Ok(ContractClass::V1Native(NativeContractClassV1::from_file(path.with_extension("sierra.json").to_str().unwrap())))
    }
}
