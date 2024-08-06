use blockifier::{
    context::BlockContext,
    declare_tx_args,
    execution::execution_utils::felt_to_stark_felt,
    invoke_tx_args,
    state::{cached_state::CachedState, state_api::StateReader},
    test_utils::{
        create_calldata, declare::declare_tx, MAX_FEE, MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE,
    },
    transaction::{
        test_utils::{account_invoke_tx, calculate_class_info_for_testing, l1_resource_bounds},
        transactions::ExecutableTransaction,
    },
};
use cairo_felt::Felt252;
use log::info;
use starknet_api::{
    core::{ClassHash, ContractAddress},
    hash::StarkFelt,
    transaction::{Calldata, Fee, TransactionVersion},
};
use utils::{create_state, get_class_hash, get_compiled_class_hash, load_contract};

pub const ACCOUNT_ADDRESS: u32 = 4321;
pub const OWNER_ADDRESS: u32 = 4321;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let cairo_native = match &*args[0] {
        "native" => true,
        "vm" => false,
        arg => {
            info!("Not a valid mode: {}, using vm", arg);
            false
        }
    };

    let mut state = create_state(cairo_native)?;
    let account_address = ContractAddress(ACCOUNT_ADDRESS.into());

    // Declare ERC20, YASFactory, YASPool and YASRouter contracts.
    info!("Declaring the ERC20 contract.");
    let erc20_class_hash = declare_contract(&mut state, "ERC20", cairo_native)?;
    info!("Declaring the YASFactory contract.");
    let _yas_factory_class_hash = declare_contract(&mut state, "YASFactory", cairo_native)?;
    info!("Declaring the YASRouter contract.");
    let _yas_router_class_hash = declare_contract(&mut state, "YASRouter", cairo_native)?;
    info!("Declaring the YASPool contract.");
    let _yas_pool_class_hash = declare_contract(&mut state, "YASPool", cairo_native)?;

    // Deploys

    let name = Felt252::from_bytes_be("TYAS0".as_bytes());
    let name = felt_to_stark_felt(&name);
    let symbol = Felt252::from_bytes_be("$YAS0".as_bytes());
    let symbol = felt_to_stark_felt(&symbol);
    let nonce = state.get_nonce_at(account_address)?;

    let calldata = create_calldata(
        account_address,
        "deploy",
        &[
            erc20_class_hash.0,
            nonce.0.into(),
            StarkFelt::from(5_u32),
            name,
            symbol,
            0x3782_dace_9d90_0000_u128.into(),
            0_u128.into(),
            OWNER_ADDRESS.into(),
        ],
    );
    // Deploy two ERC20 contracts.
    info!("Deploying TYAS0 token on ERC20.");
    let _yas0_token_address = deploy_contract(&mut state, calldata)?;
    dbg!("deploy 1");

    let name = Felt252::from_bytes_be("TYAS1".as_bytes());
    let name = felt_to_stark_felt(&name);
    let symbol = Felt252::from_bytes_be("$YAS1".as_bytes());
    let symbol = felt_to_stark_felt(&symbol);
    let nonce = state.get_nonce_at(account_address)?;

    let calldata = create_calldata(
        account_address,
        "deploy",
        &[
            erc20_class_hash.0,
            nonce.0.into(),
            5_u32.into(),
            name,
            symbol,
            0x3782_dace_9d90_0000_u128.into(),
            0_u128.into(),
            OWNER_ADDRESS.into(),
        ],
    );
    info!("Deploying TYAS1 token on ERC20.");
    let _yas1_token_address = deploy_contract(&mut state, calldata)?;
    dbg!("deploy 2");

    Ok(())
}

fn declare_contract<S: StateReader>(
    mut state: &mut CachedState<S>,
    contract_name: &str,
    cairo_native: bool
) -> Result<ClassHash, Box<dyn std::error::Error>> {
    let contract_class = load_contract(contract_name, cairo_native)?;
    let block_context = &BlockContext::create_for_testing();
    let class_info = calculate_class_info_for_testing(contract_class);
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let class_hash = get_class_hash(contract_name);
    let compiled_class_hash = get_compiled_class_hash(contract_name);
    let nonce = state.get_nonce_at(sender_address)?;
    let declare_args = declare_tx_args! {
        max_fee: Fee(MAX_FEE),
        sender_address,
        version: TransactionVersion::THREE,
        resource_bounds: l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE),
        class_hash,
        compiled_class_hash,
        nonce
    };

    declare_tx(declare_args, class_info.clone()).execute(&mut state, block_context, false, true)?;

    let contract_class_from_state = state.get_compiled_contract_class(class_hash).unwrap();
    assert_eq!(contract_class_from_state, class_info.contract_class());

    Ok(class_hash)
}

fn deploy_contract<S: StateReader>(
    state: &mut CachedState<S>,
    calldata: Calldata,
) -> Result<StarkFelt, Box<dyn std::error::Error>> {
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let nonce = state.get_nonce_at(sender_address)?;
    let invoke_args = invoke_tx_args!(
        nonce, 
        sender_address, 
        max_fee: Fee(MAX_FEE), 
        calldata, 
        version: TransactionVersion::THREE,
    );
    let block_context = BlockContext::create_for_account_testing();
    let execution = account_invoke_tx(invoke_args).execute(state, &block_context, false, true)?;

    dbg!(execution.revert_error.unwrap());

    let exec_call_info: blockifier::execution::call_info::CallInfo =
        execution.execute_call_info.unwrap();
    let ret = exec_call_info.execution.retdata.0[0];

    Ok(ret)
}

mod utils {
    use std::{collections::HashMap, fs, path::Path};

    use blockifier::{
        compiled_class_hash, execution::contract_class::{ContractClass, ContractClassV1, SierraContractClassV1}, state::cached_state::CachedState, test_utils::dict_state_reader::DictStateReader
    };
    use starknet_api::{
        class_hash,
        core::{ClassHash, CompiledClassHash, ContractAddress, Nonce},
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

    pub fn get_compiled_class_hash(contract: &str) -> CompiledClassHash {
        compiled_class_hash!(integer_base(contract))
    }

    pub fn create_state(cairo_native: bool) -> Result<CachedState<DictStateReader>, Box<dyn std::error::Error>> {
        let mut class_hash_to_class = HashMap::new();
        let mut address_to_class_hash = HashMap::new();
        let mut address_to_nonce = HashMap::new();
        let mut class_hash_to_compiled_class_hash = HashMap::new();

        let contract_class = load_contract("YasCustomAccount", cairo_native)?;
        let class_hash = get_class_hash("YasCustomAccount");
        let compiled_class_hash = get_compiled_class_hash("YasCustomAccount");
        
        address_to_class_hash.insert(ContractAddress(ACCOUNT_ADDRESS.into()), class_hash);
        class_hash_to_class.insert(class_hash, contract_class);
        address_to_nonce
            .insert(ContractAddress(ACCOUNT_ADDRESS.into()), Nonce(StarkFelt::from_u128(1)));
        class_hash_to_compiled_class_hash.insert(class_hash, compiled_class_hash);

        let state_reader = DictStateReader {
            class_hash_to_class,
            address_to_class_hash,
            address_to_nonce,
            class_hash_to_compiled_class_hash,
            ..Default::default()
        };

        Ok(CachedState::new(state_reader))
    }

    pub fn load_contract(name: &str, cairo_native: bool) -> Result<ContractClass, Box<dyn std::error::Error>> {
        let path = Path::new(BENCH_YAS).join(name);

        if !cairo_native {
            let casm_json = &fs::read_to_string(path.with_extension("json"))?;
            Ok(ContractClass::V1(ContractClassV1::try_from_json_string(&casm_json)?))
        } else {
            let sierra_json = &fs::read_to_string(path.with_extension("sierra.json"))?;
            Ok(ContractClass::V1Sierra(SierraContractClassV1::try_from_json_string(&sierra_json)?))
        }
    }
}
