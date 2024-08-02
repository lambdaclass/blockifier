use blockifier::{
    context::BlockContext, declare_tx_args, execution::execution_utils::felt_to_stark_felt, invoke_tx_args, state::{cached_state::CachedState, state_api::StateReader}, test_utils::{
        create_calldata, declare::declare_tx, MAX_FEE, MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE
    }, transaction::{
        test_utils::{account_invoke_tx, calculate_class_info_for_testing, l1_resource_bounds},
        transactions::ExecutableTransaction,
    }
};
use cairo_felt::Felt252;
use log::info;
use starknet_api::{
    core::{ClassHash, ContractAddress}, hash::StarkFelt, transaction::{Fee, TransactionVersion}
};
use utils::{create_state, get_sierra_contract_hash, load_contract};

pub const ACCOUNT_ADDRESS: u32 = 4321;
pub const OWNER_ADDRESS: u32 = 4321;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut state = create_state()?;

    // Declare ERC20, YASFactory, YASPool and YASRouter contracts.
    info!("Declaring the ERC20 contract.");
    let erc20_class_hash = declare_contract(&mut state, "ERC20")?;
    info!("Declaring the YASFactory contract.");
    let _yas_factory_class_hash = declare_contract(&mut state, "YASFactory")?;
    info!("Declaring the YASRouter contract.");
    let _yas_router_class_hash = declare_contract(&mut state, "YASRouter")?;
    info!("Declaring the YASPool contract.");
    let _yas_pool_class_hash = declare_contract(&mut state, "YASPool")?;

    // Deploy two ERC20 contracts.
    info!("Deploying TYAS0 token on ERC20.");
    let _yas0_token_address = deploy_erc20(
        &mut state,
        &erc20_class_hash,
        "TYAS0",
        "$YAS0",
        (0x3782_dace_9d90_0000, 0),
        OWNER_ADDRESS.into(),
    )?;
    dbg!("deploy 1");
    info!("Deploying TYAS1 token on ERC20.");
    let _yas1_token_address = deploy_erc20(
        &mut state,
        &erc20_class_hash,
        "TYAS1",
        "$YAS1",
        (0x3782_dace_9d90_0000, 0),
        OWNER_ADDRESS.into(),
    )?;
    dbg!("deploy 2");

    Ok(())
}

fn declare_contract<S: StateReader>(
    mut state: &mut CachedState<S>,
    contract_name: &str,
) -> Result<ClassHash, Box<dyn std::error::Error>> {
    let (_, casm_contract) = load_contract(contract_name)?;
    let block_context = &BlockContext::create_for_account_testing_with_kzg(false);
    let class_info = calculate_class_info_for_testing(casm_contract);
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let casm_class_hash = get_sierra_contract_hash(contract_name)?;
    let nonce = state.get_nonce_at(sender_address)?;
    let declare_args = declare_tx_args! {
        max_fee: Fee(MAX_FEE),
        sender_address,
        version: TransactionVersion::TWO,
        resource_bounds: l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE),
        class_hash: casm_class_hash,
        nonce
    };

    declare_tx(declare_args, class_info.clone()).execute(&mut state, block_context, false, true)?;

    Ok(casm_class_hash)
}

fn deploy_erc20<S: StateReader>(
    state: &mut CachedState<S>,
    _erc20_class_hash: &StarkFelt,
    name: &str,
    symbol: &str,
    initial_supply: (u128, u128),
    recipient: StarkFelt,
) -> Result<StarkFelt, Box<dyn std::error::Error>> {
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let nonce = state.get_nonce_at(sender_address)?;
    let name = Felt252::from_bytes_be(name.as_bytes());
    let name = felt_to_stark_felt(&name);
    let symbol = Felt252::from_bytes_be(symbol.as_bytes());
    let symbol = felt_to_stark_felt(&symbol);

    let calldata = create_calldata(
        sender_address,
        "deploy",
        &[
            StarkFelt::default(),
            nonce.0.into(),
            StarkFelt::from(5_u32),
            name,
            symbol,
            initial_supply.0.into(),
            initial_supply.1.into(),
            recipient,
        ],
    );
    let invoke_args = invoke_tx_args!(nonce, sender_address, max_fee: Fee(MAX_FEE), calldata);
    let block_context = &BlockContext::create_for_account_testing_with_kzg(false);

    let execution = account_invoke_tx(invoke_args).execute(state, block_context, false, false)?;

    dbg!(execution.revert_error.unwrap());

    let exec_call_info = execution.execute_call_info.unwrap();
    let ret = exec_call_info.execution.retdata.0[0];

    Ok(ret)
}

mod utils {
    use std::{collections::HashMap, fs, path::Path};

    use blockifier::{
        execution::contract_class::{ContractClass, ContractClassV1, SierraContractClassV1},
        state::cached_state::CachedState,
        test_utils::dict_state_reader::DictStateReader,
    };
    use starknet_api::{
        class_hash,
        core::{ClassHash, ContractAddress, Nonce},
        hash::{StarkFelt, StarkHash},
    };

    use crate::ACCOUNT_ADDRESS;

    const BENCH_YAS: &str = "bench/yas/";
    const CLASS_HASH_BASE: u32 = 1 << 16;
    const YAS_CUSTOM_ACCOUNT_BASE: u32 = CLASS_HASH_BASE;
    const YAS_FACTORY_BASE: u32 = 2 * CLASS_HASH_BASE;
    const YAS_POOL_BASE: u32 = 3 * CLASS_HASH_BASE;
    const YAS_ROUTER_BASE: u32 = 4 * CLASS_HASH_BASE;
    const YAS_ERC_BASE: u32 = 5 * CLASS_HASH_BASE;

    const YAS_CUSTOM_ACCOUNT_SIERRA_BASE: u32 = 6 * CLASS_HASH_BASE;
    const YAS_FACTORY_SIERRA_BASE: u32 = 7 * CLASS_HASH_BASE;
    const YAS_POOL_SIERRA_BASE: u32 = 8 * CLASS_HASH_BASE;
    const YAS_ROUTER_SIERRA_BASE: u32 = 9 * CLASS_HASH_BASE;
    const YAS_ERC_SIERRA_BASE: u32 = 10 * CLASS_HASH_BASE;

    pub fn get_sierra_contract_hash(contract: &str) -> Result<ClassHash, Box<dyn std::error::Error>> {
        let cairo1_bit = 1 << 31;
        let base = match contract {
            "YasCustomAccount" => YAS_CUSTOM_ACCOUNT_SIERRA_BASE,
            "ERC20" => YAS_ERC_SIERRA_BASE,
            "YASFactory" => YAS_FACTORY_SIERRA_BASE,
            "YASPool" => YAS_POOL_SIERRA_BASE,
            "YASRouter" => YAS_ROUTER_SIERRA_BASE,
            _ => 1
        };

        Ok(class_hash!(base + cairo1_bit))
    }

    pub fn get_casm_contract_hash(contract: &str) -> Result<ClassHash, Box<dyn std::error::Error>> {
        let cairo1_bit = 1 << 31_i32;
        let base = match contract {
            "YasCustomAccount" => YAS_CUSTOM_ACCOUNT_BASE,
            "ERC20" => YAS_ERC_BASE,
            "YASFactory" => YAS_FACTORY_BASE,
            "YASPool" => YAS_POOL_BASE,
            "YASRouter" => YAS_ROUTER_BASE,
            _ => 1,
        };

        Ok(class_hash!(base + cairo1_bit))
    }

    pub fn create_state() -> Result<CachedState<DictStateReader>, Box<dyn std::error::Error>> {
        let mut class_hash_to_class = HashMap::new();
        let mut address_to_class_hash = HashMap::new();
        let mut address_to_nonce = HashMap::new();

        let (_, casm_contract) = load_contract("YasCustomAccount")?;
        let casm_class_hash = get_casm_contract_hash("YasCustomAccount")?;
        //let sierra_class_hash = get_sierra_contract_hash("YasCustomAccountx")?;

        address_to_class_hash.insert(ContractAddress(ACCOUNT_ADDRESS.into()), casm_class_hash);
        class_hash_to_class.insert(casm_class_hash, casm_contract);
        address_to_nonce
            .insert(ContractAddress(ACCOUNT_ADDRESS.into()), Nonce(StarkFelt::from_u128(0)));

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
    ) -> Result<(ContractClass, ContractClass), Box<dyn std::error::Error>> {
        let path = Path::new(BENCH_YAS).join(name);
        let sierra_json = &fs::read_to_string(path.with_extension("sierra.json"))?;
        let casm_json = &fs::read_to_string(path.with_extension("json"))?;

        let sierra_contract =
            ContractClass::V1Sierra(SierraContractClassV1::try_from_json_string(&sierra_json)?);
        let casm_contract = ContractClass::V1(ContractClassV1::try_from_json_string(&casm_json)?);

        Ok((sierra_contract, casm_contract))
    }
}
