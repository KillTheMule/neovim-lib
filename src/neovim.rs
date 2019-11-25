use crate::rpc::*;
use crate::session::Session;
use crate::uioptions::UiAttachOptions;
use crate::callerror::{CallError, map_generic_error};
use async_std::task;

pub struct Neovim {
    pub session: Session,
}

impl Neovim {
    pub fn new(session: Session) -> Neovim {
        Neovim { session }
    }

    /// Register as a remote UI.
    ///
    /// After this method is called, the client will receive redraw notifications.
    pub fn ui_attach(
        &mut self,
        width: i64,
        height: i64,
        opts: &UiAttachOptions,
    ) -> Result<(), CallError> {
        task::block_on(self.session
            .call(
                "nvim_ui_attach",
                call_args!(width, height, opts.to_value_map()),
            )
        )
        .map_err(map_generic_error)
        .map(|_| ())
    }

    /// Send a quit command to Nvim.
    /// The quit command is 'qa!' which will make Nvim quit without
    /// saving anything.
    pub fn quit_no_save(&mut self) -> Result<(), CallError> {
        task::block_on(self.command("qa!"))
    }
}

