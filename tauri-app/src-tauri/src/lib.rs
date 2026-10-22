// Copyright (c) 2026 mba-x86. Released under the MIT License (see LICENSE).
//! Process Killer 2.0 backend. One module per UI domain; the command list is registered below (orchestrator-owned).
mod activity;
mod jobs;
mod protect;
mod recycle;
mod shell;
mod state;
mod util;

// domain modules (each owned by one backend agent; see docs/RUST_GUIDE.md)
mod apps;
mod cleanup;
mod perf;
mod processes;
mod services;
mod startup;
mod storage;
mod system;
mod updates;

pub fn run() {
    if !util::acquire_single_instance() {
        return;
    }
    tauri::Builder::default()
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            shell::app_ready,
            shell::get_overview,
            shell::relaunch_as_admin,
            // system (live data, drives, top lists, specs, directx, cpu detail, clipboard)
            system::get_live,
            system::get_drives,
            system::get_top_processes,
            system::get_top_apps,
            system::copy_text,
            system::get_specs_fast,
            system::get_specs_full,
            system::get_directx,
            system::get_cpu_detail,
            // apps (list, related, uninstall engine, leftovers)
            apps::list_apps,
            apps::get_related,
            apps::other_browsers,
            apps::uninstall_start,
            apps::find_leftovers,
            apps::clean_leftovers,
            apps::open_folder,
            // cleanup (RAM boost, junk cleaner) + storage (scan, recycle, folder picker)
            cleanup::boost_ram,
            cleanup::get_junk_categories,
            cleanup::scan_junk,
            cleanup::clean_start,
            storage::storage_scan_start,
            storage::storage_scan_cancel,
            storage::choose_folder,
            storage::recycle_files,
            // updates (activity, settings, about, updater)
            updates::get_activity,
            updates::clear_activity,
            updates::open_log_file,
            updates::get_settings,
            updates::set_settings,
            updates::get_about,
            updates::open_url,
            updates::check_update,
            updates::start_update,
            // perf (memory / network / gpu detail)
            perf::get_mem_detail,
            perf::get_mem_hw,
            perf::get_net_detail,
            perf::get_gpu_detail,
            // manage (services / processes / startup; open_location is shared with storage)
            services::list_services,
            services::set_service_state,
            processes::list_processes,
            processes::kill_process,
            processes::open_location,
            startup::list_startup,
            startup::remove_startup,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Process Killer");
}
