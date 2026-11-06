# corundum-framework-fs

An app server's workspace folder over the `fs.*` methods of `corundum-framework-fs-api`.
Paths are checked and never pass a symlink. Every dot-name and every file the app keeps
for itself (its sidecars) is hidden. Folders are listed a page at a time, with the app's
managed entries marked, and files are read in ranges.

A server mounts `FsService::dispatch` on its own JSON-RPC router, over HTTP or a local
socket.
