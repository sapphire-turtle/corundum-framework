# corundum-framework-fs-api

The `fs.*` methods an app's server serves over its workspace folder — `fs.list`, `fs.stat`
and `fs.read` — as plain serde data: their names, parameters, results and error codes.

The engine that serves them is `corundum-framework-fs`. A client depends on this crate
alone. The transport is the app's (asteria's HTTP `/rpc`, sapphire's local socket).
