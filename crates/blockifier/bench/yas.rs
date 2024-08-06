use blockifier::{
    abi::abi_utils::selector_from_name,
    context::BlockContext,
    declare_tx_args,
    execution::native::utils::stark_felt_to_native_felt,
    invoke_tx_args,
    state::{cached_state::CachedState, state_api::StateReader},
    test_utils::{
        create_calldata, declare::declare_tx, deploy_contract, dict_state_reader::DictStateReader,
        MAX_FEE, MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE,
    },
    transaction::{
        test_utils::{calculate_class_info_for_testing, l1_resource_bounds, run_invoke_tx},
        transactions::ExecutableTransaction,
    },
};
use log::info;
use num_traits::FromPrimitive;
use starknet_api::{
    core::{ClassHash, ContractAddress},
    hash::StarkFelt,
    transaction::{Fee, TransactionVersion},
};
use starknet_types_core::felt::Felt;
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
    dbg!("Declaring the ERC20 contract.");
    let erc20_class_hash = declare_contract(&mut state, "ERC20", cairo_native)?;
    dbg!("Declaring the YASFactory contract.");
    let yas_factory_class_hash = declare_contract(&mut state, "YASFactory", cairo_native)?;
    dbg!("Declaring the YASRouter contract.");
    let yas_router_class_hash = declare_contract(&mut state, "YASRouter", cairo_native)?;
    dbg!("Declaring the YASPool contract.");
    let yas_pool_class_hash = declare_contract(&mut state, "YASPool", cairo_native)?;

    // Deploys

    // Deploy two ERC20 contracts.
    let name = Felt::from_bytes_be_slice("TYAS0".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS0".as_bytes());
    let _nonce = state.get_nonce_at(account_address)?;

    let calldata =
        vec![name, symbol, 0x3782_dace_9d90_0000_u128.into(), 0_u128.into(), OWNER_ADDRESS.into()];

    dbg!("Deploying TYAS0 token on ERC20.");
    let yas0_token_address =
        tx_deploy_contract(&mut state, &calldata, stark_felt_to_native_felt(erc20_class_hash.0))?;

    let name = Felt::from_bytes_be_slice("TYAS1".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS1".as_bytes());
    let _nonce = state.get_nonce_at(account_address)?;

    let calldata =
        vec![name, symbol, 0x3782_dace_9d90_0000_u128.into(), 0_u128.into(), OWNER_ADDRESS.into()];

    dbg!("Deploying TYAS1 token on ERC20.");
    let yas1_token_address =
        tx_deploy_contract(&mut state, &calldata, stark_felt_to_native_felt(erc20_class_hash.0))?;

    let calldata = vec![stark_felt_to_native_felt(yas_pool_class_hash.0), OWNER_ADDRESS.into()];

    dbg!("Deploying YASFactory contract.");
    let yas_factory_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_factory_class_hash.0),
    )?;

    let calldata = vec![];

    dbg!("Deploying YASRouter contract.");
    let _yas_router_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_router_class_hash.0),
    )?;

    let calldata = vec![
        yas_factory_address,
        yas0_token_address,
        yas1_token_address,
        0x0bb8.into(),
        0x3c.into(),
        0.into(),
    ];

    dbg!("Deploying YASPool contract.");
    let _yas_factory_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_pool_class_hash.0),
    )?;

    dbg!("Initializing Pool");
    initialize_pool(
        &mut state,
        yas_pool_class_hash.0,
        (79_228_162_514_263_337_593543_950_336, 0),
        false,
    )?;

    Ok(())
}

fn declare_contract<S: StateReader>(
    mut state: &mut CachedState<S>,
    contract_name: &str,
    cairo_native: bool,
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

fn tx_deploy_contract<S: StateReader>(
    state: &mut CachedState<S>,
    calldata: &[Felt],
    class_hash: Felt,
) -> Result<Felt, Box<dyn std::error::Error>> {
    let (address, _): (Felt, Vec<Felt>) =
        deploy_contract(state, class_hash, Felt::from_i8(0).unwrap(), calldata).unwrap();

    Ok(address)
}

fn initialize_pool(
    state: &mut CachedState<DictStateReader>,
    pool_address: StarkFelt,
    price_sqrt: (u128, u128),
    sign: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let sender_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let nonce = state.get_nonce_at(sender_address)?;
    let calldata = create_calldata(
        sender_address,
        "__execute__",
        &[
            1_u32.into(),
            pool_address,
            selector_from_name("initialize").0,
            price_sqrt.0.into(),
            price_sqrt.1.into(),
            u32::from(sign).into(),
        ],
    );
    let args = invoke_tx_args!(nonce, sender_address, max_fee: Fee::default(), version: TransactionVersion::ONE, calldata);
    let block_context = BlockContext::create_for_account_testing();

    run_invoke_tx(state, &block_context, args)?;

    Ok(())
}

mod utils {
    use std::{collections::HashMap, fs, path::Path};

    use blockifier::{
        compiled_class_hash,
        execution::contract_class::{ContractClass, ContractClassV1, SierraContractClassV1},
        state::cached_state::CachedState,
        test_utils::dict_state_reader::DictStateReader,
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

    pub fn create_state(
        cairo_native: bool,
    ) -> Result<CachedState<DictStateReader>, Box<dyn std::error::Error>> {
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

    pub fn load_contract(
        name: &str,
        cairo_native: bool,
    ) -> Result<ContractClass, Box<dyn std::error::Error>> {
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
