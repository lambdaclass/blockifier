/*
    Usage:
        * There are two modes of running: vm and native, which should be specified as args
        * Example with VM:
            cargo bench --bench yas vm
        * Example with native:
            cargo bench --features native_jit --bench yas native (cairo native JIT)
            cargo bench --bench yas native (cairo native AOT)
        * If no args were specified then vm would be used
*/

use std::time::{Duration, Instant};

use blockifier::test_utils::yas_test_utils::{
        create_state, declare_all_contracts, deploy_all_contracts, get_balance, invoke_func,
    };
use log::{debug, info};
use starknet_api::{hash::StarkHash, transaction::Calldata};

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
    let mut consumed_gas = 0;

    let mut state = create_state(cairo_native)?;

    // Declares

    let (erc20_class_hash, yas_factory_class_hash, yas_router_class_hash, yas_pool_class_hash) =
        declare_all_contracts(&mut state, cairo_native)?;

    // Deploys

    let (yas0_token_address, yas1_token_address, yas_router_address, yas_pool_address) =
        deploy_all_contracts(
            &mut state,
            erc20_class_hash,
            yas_factory_class_hash,
            yas_router_class_hash,
            yas_pool_class_hash,
        )?;

    // Invokes

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

    info!("Approving tokens");
    let calldata = Calldata(
        vec![StarkHash::from(yas_router_address), u128::MAX.into(), u128::MAX.into()]
            .into(),
    );
    invoke_func(&mut state, "approve", yas0_token_address, calldata)?;

    let calldata = Calldata(
        vec![StarkHash::from(yas_router_address), u128::MAX.into(), u128::MAX.into()]
            .into(),
    );
    invoke_func(&mut state, "approve", yas1_token_address, calldata)?;

    info!("Minting tokens.");
    let tick_lower = -887_220_i32;
    let tick_upper = 887_220_i32;

    let calldata = Calldata(
        vec![
            StarkHash::from(yas_pool_address),
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

    let initial_balance_yas0 =
    StarkHash::from(get_balance(&mut state, yas0_token_address)?);
    let initial_balance_yas1 =
    StarkHash::from(get_balance(&mut state, yas1_token_address)?);

    debug!("TYAS0 balance: {}: ", initial_balance_yas0);
    debug!("TYAS1 balance: {}: ", initial_balance_yas1);

    let mut delta_t = Duration::ZERO;
    let mut runs = 0;

    loop {
        let calldata = Calldata(
            vec![
                StarkHash::from(yas_pool_address),
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

        info!("Swapping tokens");
        let t0 = Instant::now();
        let call_info = invoke_func(&mut state, "swap", yas_router_address, calldata)?;
        let t1 = Instant::now();

        consumed_gas += call_info.execution.gas_consumed;

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
        #[cfg(not(feature = "native_jit"))]
        {
            "Cario Native AOT"
        }
        #[cfg(feature = "native_jit")]
        {
            "Cario Native JIT"
        }
    } else {
        "Cairo VM"
    };

    println!(
        "[{bench_mode}] Executed {runs} swaps taking {delta_t} seconds ({} #/s, or {} s/#), consuming {} units of gas",
        f64::from(runs) / delta_t,
        delta_t / f64::from(runs),
        consumed_gas
    );

    Ok(())
}
