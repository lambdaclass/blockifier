use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use super::syscall_handler::NativeSyscallHandler;
use crate::execution::call_info::CallInfo;
use crate::execution::contract_class::NativeContractClassV1;
use crate::execution::entry_point::{
    CallEntryPoint, EntryPointExecutionContext, EntryPointExecutionResult,
};
use crate::execution::native::utils::{run_native_executor, run_sierra_emu_executor};
use crate::state::state_api::State;

pub fn execute_entry_point_call(
    call: CallEntryPoint,
    contract_class: NativeContractClassV1,
    state: &mut dyn State,
    resources: &mut ExecutionResources,
    context: &mut EntryPointExecutionContext,
) -> EntryPointExecutionResult<CallInfo> {
    let function_id =
        contract_class.get_entrypoint(call.entry_point_type, call.entry_point_selector)?;

    let mut syscall_handler: NativeSyscallHandler<'_> = NativeSyscallHandler::new(
        state,
        call.caller_address,
        call.storage_address,
        call.entry_point_selector,
        resources,
        context,
    );

    let result = if cfg!(feature = "use-sierra-emu") {
        let vm = sierra_emu::VirtualMachine::new_starknet(
            Arc::new(contract_class.program.clone()),
            &mut syscall_handler,
        );
        run_sierra_emu_executor(vm, function_id, call.clone())?
    } else {
        #[cfg(feature = "with-trace-dump")]
        {
            use crate::execution::native::utils::TRACE_COUNTER;
            use cairo_lang_sierra::program_registry::ProgramRegistry;
            use cairo_native::runtime::trace_dump::TraceDump;
            use cairo_native::types::TypeBuilder;
            use std::{collections::HashMap, sync::Mutex};

            // Since the library is statically linked, then dynamically loaded, each instance of
            // `TRACE_DUMP` for each contract is separate (probably). That's why we need this
            // getter and cannot use `cairo_native::runtime::TRACE_DUMP` directly.
            let trace_dump = unsafe {
                let fn_ptr = contract_class
                    .executor
                    .library
                    .get::<extern "C" fn() -> &'static Mutex<HashMap<u64, TraceDump>>>(
                        b"get_trace_dump_ptr\0",
                    )
                    .unwrap();

                fn_ptr()
            };

            // Since libraries can be shared between executions, we must increment our TRACE_COUNTER to avoid collisions
            let counter_value = TRACE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            // Insert the trace dump for this execution
            trace_dump.lock().unwrap().insert(
                counter_value as u64,
                TraceDump::new(
                    ProgramRegistry::new(&contract_class.program).unwrap(),
                    |x, registry| x.layout(registry).unwrap(),
                ),
            );

            // Set the active trace id
            let trace_id_ref = unsafe {
                contract_class
                    .executor
                    .library
                    .get::<u64>(b"TRACE_DUMP__TRACE_ID\0")
                    .unwrap()
                    .try_as_raw_ptr()
                    .unwrap()
                    .cast::<u64>()
                    .as_mut()
                    .unwrap()
            };
            let old_counter_value = *trace_id_ref;
            *trace_id_ref = counter_value as u64;

            println!("Execution started for trace #{counter_value}.");

            let result = run_native_executor(
                &contract_class.executor,
                function_id,
                call,
                syscall_handler,
                counter_value,
            )?;

            // Set old trace id in case this is a recursive call
            *trace_id_ref = old_counter_value as u64;

            println!("Execution finished for trace #{counter_value}.");

            result
        }

        #[cfg(not(feature = "with-trace-dump"))]
        run_native_executor(&contract_class.executor, function_id, call, syscall_handler)?
    };

    Ok(result)
}
