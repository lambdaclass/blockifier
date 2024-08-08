use std::{sync::Arc, time::{Duration, Instant}, u64};

use blockifier::{
    abi::abi_utils::selector_from_name,
    context::{BlockContext, TransactionContext},
    declare_tx_args,
    execution::{
        common_hints::ExecutionMode,
        entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext},
        execution_utils::execute_entry_point_call,
        native::utils::{native_felt_to_stark_felt, stark_felt_to_native_felt},
    },
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
use log::{debug, info};
use starknet_api::{
    core::{ClassHash, ContractAddress},
    deprecated_contract_class::EntryPointType,
    hash::StarkFelt,
    transaction::{Calldata, Fee, TransactionVersion},
};
use starknet_types_core::felt::Felt;
use utils::{create_state, get_balance, get_class_hash, load_contract};

pub const ACCOUNT_ADDRESS: u32 = 4321;
pub const OWNER_ADDRESS: u32 = 4321;
const WARMUP_TIME: Duration = Duration::from_secs(3);
const BENCHMARK_TIME: Duration = Duration::from_secs(5);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let cairo_native = match &*args[1] {
        "native" => true,
        "vm" | "" => false,
        arg => {
            info!("Not a valid mode: {}, using vm", arg);
            false
        }
    };

    let mut state = create_state(cairo_native)?;

    // Declare ERC20, YASFactory, YASPool and YASRouter contracts.
    info!("Declaring the ERC20 contract.");
    let erc20_class_hash = declare_contract(&mut state, "ERC20", cairo_native)?;
    info!("Declaring the YASFactory contract.");
    let yas_factory_class_hash = declare_contract(&mut state, "YASFactory", cairo_native)?;
    info!("Declaring the YASRouter contract.");
    let yas_router_class_hash = declare_contract(&mut state, "YASRouter", cairo_native)?;
    info!("Declaring the YASPool contract.");
    let yas_pool_class_hash = declare_contract(&mut state, "YASPool", cairo_native)?;

    // Deploys

    // Deploy two ERC20 contracts.
    let name = Felt::from_bytes_be_slice("TYAS0".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS0".as_bytes());

    info!("Deploying TYAS0 token on ERC20.");
    let calldata =
    vec![name, symbol, 0_u128.into(), 0x3782_dace_9d90_0000_u128.into(), OWNER_ADDRESS.into()];
    let yas0_token_address =
        tx_deploy_contract(&mut state, &calldata, stark_felt_to_native_felt(erc20_class_hash.0))?;

    let name = Felt::from_bytes_be_slice("TYAS1".as_bytes());
    let symbol = Felt::from_bytes_be_slice("$YAS1".as_bytes());

    info!("Deploying TYAS1 token on ERC20.");
    let calldata =
        vec![name, symbol, 0_u128.into(), 0x3782_dace_9d90_0000_u128.into(), OWNER_ADDRESS.into()];
    let yas1_token_address =
        tx_deploy_contract(&mut state, &calldata, stark_felt_to_native_felt(erc20_class_hash.0))?;

        info!("Deploying YASFactory contract.");
    let calldata = vec![OWNER_ADDRESS.into(), stark_felt_to_native_felt(yas_pool_class_hash.0)];
    let yas_factory_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_factory_class_hash.0),
    )?;

    info!("Deploying YASRouter contract.");
    let calldata = vec![];
    let yas_router_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_router_class_hash.0),
    )?;

    info!("Deploying YASPool contract.");
    let calldata = vec![
        yas_factory_address,
        yas0_token_address,
        yas1_token_address,
        0x0bb8.into(),
        0x3c.into(),
        0.into(),
    ];
    let yas_pool_address = tx_deploy_contract(
        &mut state,
        &calldata,
        stark_felt_to_native_felt(yas_pool_class_hash.0),
    )?;

    info!("Initializing Pool");
    let calldata = Calldata(
        vec![
            79_228_162_514_264_337_593_543_950_336_u128.into(),
            0_u128.into(),
            u32::from(false).into(),
        ]
        .into(),
    );
    invoke_func(&mut state, "initialize", yas_pool_address, calldata)?;

    debug!("TYAS0 balance: {}: ", get_balance(&mut state, yas0_token_address)?);

    debug!("TYAS1 balance: {}: ", get_balance(&mut state, yas1_token_address)?);

    info!("Approving tokens");
    let calldata = Calldata(
        vec![
            native_felt_to_stark_felt(yas_router_address),
            u128::MAX.into(),
            u128::MAX.into(),
        ]
        .into(),
    );
    invoke_func(&mut state, "approve", yas0_token_address, calldata)?;

    let calldata = Calldata(
        vec![
            native_felt_to_stark_felt(yas_router_address),
            u128::MAX.into(),
            u128::MAX.into(),
        ]
        .into(),
    );
    invoke_func(&mut state, "approve", yas1_token_address, calldata)?;

    debug!("TYAS0 balance: {}: ", get_balance(&mut state, yas0_token_address)?);

    debug!("TYAS1 balance: {}: ", get_balance(&mut state, yas1_token_address)?);

    info!("Minting tokens.");
    let tick_lower = -887_220_i32;
    let tick_upper = 887_220_i32;

    let calldata = Calldata(
        vec![
            native_felt_to_stark_felt(yas_pool_address),
            OWNER_ADDRESS.into(),
            tick_lower.unsigned_abs().into(),
            u32::from(tick_lower.is_negative()).into(),
            tick_upper.unsigned_abs().into(),
            u32::from(tick_upper.is_negative()).into(),
            2_000_000_000_000_000_000_u128.into(),
        ]
        .into(),
    );
    invoke_func(&mut state, "mint", yas_router_address, calldata.clone())?;

    let mut delta_t = Duration::ZERO;
    let mut runs = 0;

    loop {
        let calldata = Calldata(
            vec![
                native_felt_to_stark_felt(yas_pool_address),
                OWNER_ADDRESS.into(),
                u32::from(true).into(),
                500_000_000_000_000_u128.into(),
                0_u32.into(),
                u32::from(true).into(),
                4_295_128_740_u128.into(),
                0_u32.into(),
                u32::from(false).into()
            ]
            .into(),
        );

        info!("Swapping tokens");
        let t0 = Instant::now();
        invoke_func(&mut state, "swap", yas_router_address, calldata)?;
        let t1 = Instant::now();

        delta_t += t1.duration_since(t0);

        if delta_t >= WARMUP_TIME {
            runs += 1;

            if delta_t >= (WARMUP_TIME + BENCHMARK_TIME) {
                break;
            }
        }
    }
    let delta_t = (delta_t - WARMUP_TIME).as_secs_f64();
    let bench_mode = if cairo_native {
        "Cario Native"
    } else {
        "Cairo VM"
    };

    println!(
        "[{}] Executed {runs} swaps taking {delta_t} seconds ({} #/s, or {} s/#): benchmark",
        bench_mode,
        f64::from(runs) / delta_t,
        delta_t / f64::from(runs),
    );

    debug!("TYAS0 balance: {}: ", get_balance(&mut state, yas0_token_address)?);

    debug!("TYAS1 balance: {}: ", get_balance(&mut state, yas1_token_address)?);

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
    let nonce = state.get_nonce_at(sender_address)?;
    let declare_args = declare_tx_args! {
        max_fee: Fee(MAX_FEE),
        sender_address,
        version: TransactionVersion::THREE,
        resource_bounds: l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE),
        class_hash,
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
    let salt = state.get_nonce_at(ContractAddress(ACCOUNT_ADDRESS.into()))?;
    let salt = stark_felt_to_native_felt(salt.0);
    let (address, _): (Felt, Vec<Felt>) =
        deploy_contract(state, class_hash, salt, calldata).unwrap();

    Ok(address)
}

fn invoke_func(
    state: &mut CachedState<DictStateReader>,
    entry_point: &str,
    contract_address: Felt,
    calldata: Calldata,
) -> Result<Vec<StarkFelt>, Box<dyn std::error::Error>> {
    let caller_address = ContractAddress(ACCOUNT_ADDRESS.into());
    let contract_address = ContractAddress(native_felt_to_stark_felt(contract_address).try_into()?);
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

    let return_data = call_info.execution.retdata.0;

    Ok(return_data)
}

mod utils {
    use std::{collections::HashMap, fs, path::Path};

    use blockifier::{
        compiled_class_hash,
        execution::{
            contract_class::{ContractClass, ContractClassV1, SierraContractClassV1},
            execution_utils::felt_to_stark_felt,
            native::utils::native_felt_to_stark_felt,
        },
        state::{cached_state::CachedState, state_api::StateReader},
        test_utils::dict_state_reader::DictStateReader,
    };
    use cairo_felt::Felt252;
    use starknet_api::{
        class_hash,
        core::{ClassHash, CompiledClassHash, ContractAddress, Nonce},
        hash::{StarkFelt, StarkHash},
    };
    use starknet_types_core::felt::Felt;

    use crate::{ACCOUNT_ADDRESS, OWNER_ADDRESS};

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

    pub fn get_balance<S: StateReader>(
        state: &mut CachedState<S>,
        token_address: Felt,
    ) -> Result<StarkFelt, Box<dyn std::error::Error>> {
        let (low, high) = state.get_fee_token_balance(
            ContractAddress(OWNER_ADDRESS.into()),
            ContractAddress(native_felt_to_stark_felt(token_address).try_into()?),
        )?;

        let low = &low.bytes()[15..];
        let high = &high.bytes()[15..];

        let balance = felt_to_stark_felt(&Felt252::from_bytes_be(&[low, high].concat()));

        Ok(balance)
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
