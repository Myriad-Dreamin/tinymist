use dapts::InitializeRequestArguments;
use sync_ls::*;
use tinymist_project::CompileFontArgs;

use crate::{Config, ServerState};

/// The regular initializer.
pub struct RegularInit {
    /// The connection to the client.
    pub client: TypedLspClient<ServerState>,
    /// The font options for the compiler.
    pub font_opts: CompileFontArgs,
}

impl Initializer for RegularInit {
    type I = InitializeRequestArguments;
    type S = ServerState;
    fn initialize(
        self,
        params: InitializeRequestArguments,
    ) -> (ServerState, AnySchedulableResponse) {
        let (config, err) = Config::extract_dap_params(params, self.font_opts);

        // if (args.supportsInvalidatedEvent) {
        //   this._useInvalidatedEvent = true;
        // }

        let super_init = SuperInit {
            client: self.client,
            config,
            err,
        };

        super_init.initialize(())
    }
}

/// The super DAP initializer.
pub struct SuperInit {
    /// Using the connection to the client.
    pub client: TypedLspClient<ServerState>,
    /// The configuration for the server.
    pub config: Config,
    /// Whether an error occurred before super initialization.
    pub err: Option<ResponseError>,
}

impl Initializer for SuperInit {
    type I = ();
    type S = ServerState;
    /// The 'initialize' request is the first request called by the frontend
    /// to interrogate the features the debug adapter provides.
    fn initialize(self, _params: ()) -> (ServerState, AnySchedulableResponse) {
        let SuperInit {
            client,
            config,
            err,
        } = self;
        // Bootstrap server
        let service = ServerState::main(client, config, err.is_none());

        if let Some(err) = err {
            return (service, Err(err));
        }

        // build and return the capabilities of this debug adapter:
        let res = dapts::Capabilities {
            supports_configuration_done_request: Some(true),

            // make client use 'evaluate' when hovering over source
            supports_evaluate_for_hovers: Some(true),
            // Don't show a 'step back' button
            supports_step_back: Some(false),
            supports_data_breakpoints: Some(true),
            // make client support completion in REPL
            supports_completions_request: Some(true),
            completion_trigger_characters: Some(vec!['.'.into(), '['.into()]),

            supports_cancel_request: Some(false),

            // make client send the breakpointLocations request
            supports_breakpoint_locations_request: Some(true),
            // make client provide "Step in Target" functionality
            supports_step_in_targets_request: Some(true),

            // the adapter defines two exceptions filters, one with support for
            // conditions.
            supports_exception_filter_options: Some(false),
            supports_exception_info_request: Some(false),
            exception_breakpoint_filters: Some(vec![
                dapts::ExceptionBreakpointsFilter {
                    filter: "layoutIterationException".into(),
                    label: "Layout Iteration Exception".into(),
                    description: Some("Break on each layout iteration.".into()),
                    default: Some(false),
                    supports_condition: Some(true),
                    condition_description: Some(
                        "Enter a typst expression to stop on specific layout iterator.
                        e.g. `iterate-step == 3 and sys.inputs.target == \"html\"`"
                            .into(),
                    ),
                },
                dapts::ExceptionBreakpointsFilter {
                    filter: "otherExceptions".into(),
                    label: "Other Exceptions".into(),
                    description: Some("This is a other exception".into()),
                    default: Some(true),
                    supports_condition: Some(false),
                    condition_description: None,
                },
            ]),

            supports_set_variable: Some(false),
            supports_set_expression: Some(false),

            // make client send disassemble request
            supports_disassemble_request: Some(false),

            supports_stepping_granularity: Some(true),
            supports_instruction_breakpoints: Some(false),

            // make client able to read and write variable memory
            supports_read_memory_request: Some(false),
            supports_write_memory_request: Some(false),

            support_suspend_debuggee: Some(true),
            support_terminate_debuggee: Some(true),
            // supports_terminate_request: Some(true),
            supports_function_breakpoints: Some(true),
            supports_delayed_stack_trace_loading: Some(true),

            ..Default::default()
        };

        let res = serde_json::to_value(res).map_err(|e| invalid_params(e.to_string()));

        // since this debug adapter can accept configuration requests like
        // 'setBreakpoint' at any time, we request them early by sending an
        // 'initializeRequest' to the frontend. The frontend will end the
        // configuration sequence by calling 'configurationDone' request.
        service
            .client
            .send_dap_event::<dapts::event::Initialized>(None);

        (service, just_result(res))
    }
}
