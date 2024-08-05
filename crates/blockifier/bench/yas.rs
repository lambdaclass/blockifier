use blockifier::{
    context::BlockContext, declare_tx_args, execution::execution_utils::felt_to_stark_felt, invoke_tx_args, state::{cached_state::CachedState, state_api::StateReader}, test_utils::{
        contracts::FeatureContract, create_calldata, declare::declare_tx, dict_state_reader::DictStateReader, CairoVersion, MAX_FEE, MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE
    }, transaction::{
        objects::FeeType, test_utils::{account_invoke_tx, calculate_class_info_for_testing, l1_resource_bounds}, transactions::ExecutableTransaction
    }
};
use cairo_felt::Felt252;
use log::info;
use starknet_api::{
    core::{ClassHash, ContractAddress},
    hash::StarkFelt,
    transaction::{Calldata, Fee, TransactionVersion},
};
use utils::{create_state, get_contract_hash, load_contract};

pub const ACCOUNT_ADDRESS: u32 = 4321;
pub const OWNER_ADDRESS: u32 = 4321;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account_1 = FeatureContract::AccountWithLongValidate(CairoVersion::Cairo1);
    let mut state = create_state(0x3782_dace_9d90_0000, &[(account_1, 2)])?;

    let account_add_1 = account_1.get_instance_address(0);
    let account_add_2 = account_1.get_instance_address(1);

    // // Declare ERC20, YASFactory, YASPool and YASRouter contracts.
    // info!("Declaring the ERC20 contract.");
    // let erc20_class_hash = declare_contract(&mut state, "ERC20")?;
    dbg!("Declaring the YASFactory contract.");
    let yas_factory_class_hash =
        declare_contract(&mut state, account_add_1, "YASFactory")?;
    dbg!("Declaring the YASRouter contract.");
    let _yas_router_class_hash =
        declare_contract(&mut state, account_add_1, "YASRouter")?;
    dbg!("Declaring the YASPool contract.");
    let yas_pool_class_hash =
        declare_contract(&mut state, account_add_1, "YASPool")?;

    balance_of(&mut state, account_add_1)?;

    // // Deploy two ERC20 contracts.
    // info!("Deploying TYAS0 token on ERC20.");
    // let _yas0_token_address = deploy_erc20(
    //     &mut state,
    //     &erc20_class_hash,
    //     "TYAS0",
    //     "$YAS0",
    //     (0x3782_dace_9d90_0000, 0),
    //     OWNER_ADDRESS.into(),
    // )?;
    // dbg!("deploy 1");
    // info!("Deploying TYAS1 token on ERC20.");
    // let _yas1_token_address = deploy_erc20(
    //     &mut state,
    //     &erc20_class_hash,
    //     "TYAS1",
    //     "$YAS1",
    //     (0x3782_dace_9d90_0000, 0),
    //     OWNER_ADDRESS.into(),
    // )?;
    // dbg!("deploy 2");

    // Deploy YASFactory contract.
    info!("Deploying YASFactory contract.");
    let calldata = create_calldata(account_add_1, "deploy", &[
        yas_factory_class_hash.0,
        state.get_nonce_at(account_add_1).unwrap().0,
        0_u32.into(),
        *account_add_2.0.key(),
        yas_pool_class_hash.0,
    ]);

    let _yas_factory_address = deploy_contract(
        &mut state,
        account_add_1,
        calldata
    )?;

    Ok(())
}

fn declare_contract<S: StateReader>(
    mut state: &mut CachedState<S>,
    sender_address: ContractAddress,
    contract_name: &str,
) -> Result<ClassHash, Box<dyn std::error::Error>> {
    let (_, casm_contract) = load_contract(contract_name)?;
    let block_context = &BlockContext::create_for_account_testing();
    let class_info = calculate_class_info_for_testing(casm_contract);
    let class_hash = get_contract_hash(contract_name)?;
    let nonce = state.get_nonce_at(sender_address)?;
    let declare_args = declare_tx_args! {
        max_fee: Fee(MAX_FEE),
        sender_address,
        version: TransactionVersion::TWO,
        resource_bounds: l1_resource_bounds(MAX_L1_GAS_AMOUNT, MAX_L1_GAS_PRICE),
        class_hash,
        nonce
    };

    declare_tx(declare_args, class_info.clone()).execute(
        &mut state,
        block_context,
        false,
        false,
    )?;

    Ok(class_hash)
}

fn deploy_contract<S: StateReader>(
    state: &mut CachedState<S>,
    contract_address: ContractAddress,
    calldata: Calldata
) -> Result<StarkFelt, Box<dyn std::error::Error>> {
    let nonce = state.get_nonce_at(contract_address)?;
    let invoke_args =
        invoke_tx_args!(nonce, sender_address: contract_address, max_fee: Fee(MAX_FEE), calldata);
    let block_context = &BlockContext::create_for_testing();

    let execution = account_invoke_tx(invoke_args).execute(state, block_context, false, false)?;

    dbg!(execution.revert_error);

    let exec_call_info = execution.execute_call_info.unwrap();
    let ret = exec_call_info.execution.retdata.0[0];

    Ok(ret)
}

pub fn balance_of(
    state: &mut CachedState<DictStateReader>,
    wallet_address: ContractAddress,
) -> Result<StarkFelt, Box<dyn std::error::Error>> {
    let fee_type = FeeType::Eth;
    let block_context = BlockContext::create_for_account_testing();
    let fee_token_address = block_context.chain_info().fee_token_address(&fee_type);
    let (low, high) = state.get_fee_token_balance(wallet_address, fee_token_address).unwrap();
    
    let balance_low = &low.bytes()[15..];
    let balance_high = &high.bytes()[15..];
    let balance_bytes = [balance_low, balance_high].concat();
    let balance = Felt252::from_bytes_be(&balance_bytes);
    let balance = felt_to_stark_felt(&balance);
    
    Ok(balance)
}

mod utils {
    use std::{fs, path::Path};

    use blockifier::{
        context::BlockContext,
        execution::contract_class::{ContractClass, ContractClassV1, SierraContractClassV1},
        state::cached_state::CachedState,
        test_utils::{
            contracts::FeatureContract, dict_state_reader::DictStateReader,
            initial_test_state::test_state,
        },
    };
    use starknet_api::{class_hash, core::ClassHash, hash::StarkHash};

    const BENCH_YAS: &str = "bench/yas/";
    const CLASS_HASH_BASE: u32 = 1 << 16;
    const YAS_CUSTOM_ACCOUNT_BASE: u32 = CLASS_HASH_BASE;
    const YAS_FACTORY_BASE: u32 = 2 * CLASS_HASH_BASE;
    const YAS_POOL_BASE: u32 = 3 * CLASS_HASH_BASE;
    const YAS_ROUTER_BASE: u32 = 4 * CLASS_HASH_BASE;
    const YAS_ERC_BASE: u32 = 5 * CLASS_HASH_BASE;

    pub fn get_contract_hash(contract: &str) -> Result<ClassHash, Box<dyn std::error::Error>> {
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

    pub fn create_state(
        initial_balances: u128,
        contract_instances: &[(FeatureContract, u16)],
    ) -> Result<CachedState<DictStateReader>, Box<dyn std::error::Error>> {
        let block_context = &BlockContext::create_for_account_testing_with_kzg(false);
        let state = test_state(&block_context.chain_info(), initial_balances, contract_instances);
        // let mut class_hash_to_class = HashMap::new();
        // let mut address_to_class_hash = HashMap::new();
        // let mut address_to_nonce = HashMap::new();

        // let (_, casm_contract) = load_contract("YasCustomAccount")?;
        // let casm_class_hash = get_contract_hash("YasCustomAccount")?;
        // //let sierra_class_hash = get_sierra_contract_hash("YasCustomAccountx")?;

        // address_to_class_hash.insert(ContractAddress(ACCOUNT_ADDRESS.into()), casm_class_hash);
        // class_hash_to_class.insert(casm_class_hash, casm_contract);
        // address_to_nonce
        //     .insert(ContractAddress(ACCOUNT_ADDRESS.into()), Nonce(StarkFelt::from_u128(1)));

        // let state_reader = DictStateReader {
        //     class_hash_to_class,
        //     address_to_class_hash,
        //     address_to_nonce,
        //     ..Default::default()
        // };

        Ok(state)
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
