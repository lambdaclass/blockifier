use blockifier::execution::native::utils::{native_felt_to_stark_felt, stark_felt_to_native_felt};
use blockifier::{
    state::cached_state::CachedState, test_utils::{dict_state_reader::DictStateReader, yas_test_utils::{create_state, declare_all_contracts, deploy_all_contracts, invoke_func, OWNER_ADDRESS}},
    
};
use starknet_api::transaction::Calldata;
use starknet_types_core::felt::Felt;

fn prepare_state(
    native: bool,
) -> Result<(CachedState<DictStateReader>, Felt, Felt, Felt, Felt), Box<dyn std::error::Error>> {
    let mut state = create_state(native).unwrap();

    // Declares
    let (erc20_class_hash, yas_factory_class_hash, yas_router_class_hash, yas_pool_class_hash) =
        declare_all_contracts(&mut state, true).unwrap();

    // Deploys
    let (yas0_token_address, yas1_token_address, yas_router_address, yas_pool_address) =
        deploy_all_contracts(
            &mut state,
            erc20_class_hash,
            yas_factory_class_hash,
            yas_router_class_hash,
            yas_pool_class_hash,
        )
        .unwrap();

    // Invokes
    let calldata = Calldata(
        vec![
            79_228_162_514_264_337_593_543_950_336_u128.into(),
            0_u128.into(),
            u32::from(false).into(),
        ]
        .into(),
    );
    invoke_func(&mut state, "initialize", yas_pool_address, calldata).unwrap();

    let calldata = Calldata(
        vec![native_felt_to_stark_felt(yas_router_address), u128::MAX.into(), u128::MAX.into()]
            .into(),
    );
    invoke_func(&mut state, "approve", yas0_token_address, calldata)?;

    let calldata = Calldata(
        vec![native_felt_to_stark_felt(yas_router_address), u128::MAX.into(), u128::MAX.into()]
            .into(),
    );
    invoke_func(&mut state, "approve", yas1_token_address, calldata)?;

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

    Ok((state, yas0_token_address, yas1_token_address, yas_router_address, yas_pool_address))
}

#[cfg(test)]
mod native_vm_tests {
    use blockifier::test_utils::yas_test_utils::get_balance;

    use super::*;

    #[test]
    fn vm_native_has_same_results() {
        let (
            mut state_vm,
            yas0_token_address_vm,
            yas1_token_address_vm,
            yas_router_address,
            yas_pool_address,
        ) = prepare_state(false).unwrap();
        let (
            mut state_native,
            yas0_token_address_native,
            yas1_token_address_native,
            _,
            _,
        ) = prepare_state(false).unwrap();

        let initial_balance_yas0 =
            stark_felt_to_native_felt(get_balance(&mut state_vm, yas0_token_address_vm).unwrap());
        let initial_balance_yas1 =
            stark_felt_to_native_felt(get_balance(&mut state_vm, yas1_token_address_vm).unwrap());

        let calldata = Calldata(
            vec![
                native_felt_to_stark_felt(yas_pool_address),
                OWNER_ADDRESS.into(),
                u32::from(true).into(),
                500_000_000_000_u128.into(),
                0_u32.into(),
                u32::from(true).into(),
                4_295_128_740_u128.into(),
                0_u32.into(),
                u32::from(false).into(),
            ]
            .into(),
        );

        invoke_func(&mut state_vm, "swap", yas_router_address, calldata.clone()).unwrap();
        invoke_func(&mut state_native, "swap", yas_router_address, calldata).unwrap();

        // Assertions

        let balance_difference_yas0_vm: u64 = native_felt_to_stark_felt(
            initial_balance_yas0
                - stark_felt_to_native_felt(get_balance(&mut state_vm, yas0_token_address_vm).unwrap()),
        )
        .try_into()
        .unwrap();

        let balance_difference_yas1_vm: u64 = native_felt_to_stark_felt(
            stark_felt_to_native_felt(get_balance(&mut state_vm, yas1_token_address_vm).unwrap())
                - initial_balance_yas1,
        )
        .try_into()
        .unwrap();

        let balance_difference_yas0_native: u64 = native_felt_to_stark_felt(
            initial_balance_yas0
                - stark_felt_to_native_felt(get_balance(&mut state_vm, yas0_token_address_native).unwrap()),
        )
        .try_into()
        .unwrap();

        let balance_difference_yas1_native: u64 = native_felt_to_stark_felt(
            stark_felt_to_native_felt(get_balance(&mut state_vm, yas1_token_address_native).unwrap())
                - initial_balance_yas1,
        )
        .try_into()
        .unwrap();

        let storages_vm = state_vm.state.storage_view;
        let storages_native = state_native.state.storage_view;

        let visited_pcs_native = state_native.visited_pcs;
        let visited_pcs_vm = state_vm.visited_pcs;

        let address_to_class_hash_map_vm = state_vm.state.address_to_class_hash;
        let address_to_class_hash_map_native = state_native.state.address_to_class_hash;

        let address_to_nonce_map_native = state_native.state.address_to_nonce;
        let address_to_nonce_map_vm = state_vm .state.address_to_nonce;

        let class_hash_to_class_map_native = state_native.state.class_hash_to_class;
        let class_hash_to_class_map_vm = state_vm.state.class_hash_to_class;

        // Balance Assertions
        assert_eq!(balance_difference_yas0_native, balance_difference_yas0_vm);
        assert_eq!(balance_difference_yas1_native, balance_difference_yas1_vm);
        
        // State Assertions
        assert_eq!(storages_native, storages_vm);   
        assert_eq!(visited_pcs_native, visited_pcs_vm);
        assert_eq!(address_to_class_hash_map_native, address_to_class_hash_map_vm);
        assert_eq!(address_to_nonce_map_native, address_to_nonce_map_vm);
        assert_eq!(class_hash_to_class_map_native, class_hash_to_class_map_vm);
    }
}
