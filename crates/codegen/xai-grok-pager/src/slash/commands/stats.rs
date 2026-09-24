//! `/stats` opens the local usage dashboard.

use crate::app::actions::Action;
use crate::slash::command::{AppCtx, CommandExecCtx, CommandResult, SlashCommand, slash_meta};

pub struct StatsCommand;

impl SlashCommand for StatsCommand {
    slash_meta! {
        name: "stats",
        aliases: ["tokstat"],
        description: "Usage and speed across local sessions",
        usage: "/stats",
    }

    fn visible(&self, _ctx: &AppCtx) -> bool {
        true
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        let arg = args.trim();
        if arg.is_empty() {
            CommandResult::Action(Action::ShowStats)
        } else {
            CommandResult::Error(format!("Unknown argument: {arg}. Use /stats"))
        }
    }
}
