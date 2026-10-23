// Copyright (C) 2026 the Cairn contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
// GPLv3 section 7 additional terms: AUTHORS.md. Unofficial, free, not affiliated with or endorsed by the studios or publishers
// of the games it supports. Portions derive from Kingdom Come: Together and kcd2-multiplayer (AUTHORS.md).
//! What a game's pack program does for the game's settings window: the toggles of its `options` (asked and applied by the program) and its saves.

use crate::app::{CairnApp, Msg};
use crate::games::GameProfile;
use cairn_setup::bridge_protocol::BridgeSaveSlot;
use cairn_setup::external_install;
use cairn_setup::game_options::{self, OptionStatus};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

impl CairnApp {
    /// The pack's program for options and saves.
    pub fn options_program(&self, game_id: &str) -> Option<PathBuf> {
        self.store.read_contract(game_id)?;
        external_install::options_program_path(&self.store, game_id).filter(|p| p.is_file())
    }

    pub fn game_has_saves(&self, game: &GameProfile) -> bool {
        game.exe_is_setting || self.saves.as_ref().is_some_and(|s| !s.is_empty())
    }

    fn options_dir(&mut self, game: &GameProfile) -> String {
        self.game_folder_of(game)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Asks the program what is so for each option, and what saves there are (when the window opens).
    pub fn load_game_extras(&mut self, game: &GameProfile) {
        self.option_status.clear();
        self.saves = None;
        self.confirm_delete_save.clear();
        self.confirm_delete_all = false;
        let Some(program) = self.options_program(&game.id) else { return };
        let dir = self.options_dir(game);
        let (tx, ctx, id, options) = (self.tx.clone(), self.ctx.clone(), game.id.clone(), game.options.clone());
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let mut found = HashMap::new();
            for o in &options {
                if let Some(st) = game_options::check(&program, &o.id, &dir, None, &cancel) {
                    found.insert(o.id.clone(), st);
                }
            }
            let saves = game_options::list_saves(&program, None, &cancel);
            let _ = tx.send(Msg::GameExtras {
                game: id,
                options: found,
                saves,
            });
            ctx.request_repaint();
        });
    }

    /// After the settings were saved: each option whose tick differs from what the program says is so is done by the program.
    pub fn apply_game_options(&mut self, game: &GameProfile) {
        let Some(program) = self.options_program(&game.id) else { return };
        let changed: Vec<(String, bool)> = game
            .options
            .iter()
            .filter_map(|o| {
                let st = self.option_status.get(&o.id)?;
                let want = self
                    .settings
                    .game_options
                    .get(&game.id)
                    .and_then(|m| m.get(&o.id))
                    .copied()
                    .unwrap_or(false);
                (st.available && st.on != want).then(|| (o.id.clone(), want))
            })
            .collect();
        if changed.is_empty() {
            return;
        }
        let dir = self.options_dir(game);
        self.options_busy = true;
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), game.id.clone());
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let results = changed
                .into_iter()
                .map(|(opt, want)| {
                    let r = game_options::apply(&program, &opt, want, &dir, &mut |_| {}, None, &cancel);
                    (opt, want, r.ok, r.message)
                })
                .collect();
            let _ = tx.send(Msg::OptionsApplied { game: id, results });
            ctx.request_repaint();
        });
    }

    pub fn on_options_applied(&mut self, game: &str, results: Vec<(String, bool, bool, String)>) {
        self.options_busy = false;
        let mut failed = false;
        for (opt, want, ok, message) in results {
            let state = if ok { want } else { !want }; // it stayed as it was
            self.settings
                .game_options
                .entry(game.to_string())
                .or_default()
                .insert(opt.clone(), state);
            self.option_status.entry(opt).and_modify(|s: &mut OptionStatus| s.on = state);
            if !ok {
                failed = true;
            }
            if !message.is_empty() {
                self.toast(&message, !ok);
            }
        }
        if failed {
            self.write_settings("an option could not be changed");
        } else {
            self.write_settings("game options");
        }
    }

    pub fn delete_save(&mut self, save_id: &str) {
        let Some(program) = self.options_program(&self.game_settings_open.clone().unwrap_or_default()) else {
            return;
        };
        self.saves_loading = true;
        let (tx, ctx, id) = (self.tx.clone(), self.ctx.clone(), save_id.to_string());
        std::thread::spawn(move || {
            let (ok, message, slots) = game_options::delete_save(&program, &id, None, &AtomicBool::new(false));
            let _ = tx.send(Msg::SavesChanged { ok, message, slots });
            ctx.request_repaint();
        });
    }

    pub fn delete_all_saves(&mut self) {
        let Some(program) = self.options_program(&self.game_settings_open.clone().unwrap_or_default()) else {
            return;
        };
        self.saves_loading = true;
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let cancel = AtomicBool::new(false);
            let (ok, message) = game_options::delete_all_saves(&program, None, &cancel);
            let slots = game_options::list_saves(&program, None, &cancel);
            let _ = tx.send(Msg::SavesChanged { ok, message, slots });
            ctx.request_repaint();
        });
    }

    pub fn on_saves_changed(&mut self, ok: bool, message: String, slots: Option<Vec<BridgeSaveSlot>>) {
        self.saves_loading = false;
        if slots.is_some() {
            self.saves = slots;
        }
        self.confirm_delete_save.clear();
        self.confirm_delete_all = false;
        self.toast(&message, !ok);
    }
}
