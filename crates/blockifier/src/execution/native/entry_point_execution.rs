use std::sync::Arc;

use cairo_vm::vm::runners::cairo_runner::ExecutionResources;

use super::syscall_handler::NativeSyscallHandler;
use crate::execution::call_info::CallInfo;
use crate::execution::contract_class::NativeContractClassV1;
use crate::execution::entry_point::{
    CallEntryPoint, EntryPointExecutionContext, EntryPointExecutionResult,
};
use crate::execution::native::utils::run_sierra_emu_executor;
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

    let syscall_handler: NativeSyscallHandler<'_> = NativeSyscallHandler::new(
        state,
        call.caller_address,
        call.storage_address,
        call.entry_point_selector,
        resources,
        context,
    );

    println!("Blockifier-Sierra-Emu: running the Native Sierra emulator");
    let vm = sierra_emu::VirtualMachine::new(Arc::new(contract_class.program.clone()));
    let result2 = run_sierra_emu_executor(vm, function_id, call.clone(), syscall_handler)?;
    dbg!(result2);
    // let result = run_native_executor(&contract_class.executor, function_id, call,
    // syscall_handler);
    println!("Blockifier-Sierra-Emu: Sierra Emu Executor finished running");
    todo!()
}
