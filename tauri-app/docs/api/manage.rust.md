# manage (services, processes, startup) - generate_handler! lines
```
services::list_services,
services::set_service_state,
processes::list_processes,
processes::kill_process,
processes::open_location,   // shared with storage
startup::list_startup,
startup::remove_startup,
```
No managed state (processes keeps a private static System). No Cargo changes.
