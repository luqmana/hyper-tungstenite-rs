# v0.3.2 - 2021-04-11
* Derive `Debug` for `HyperWebsocket` to facilitate debugging.

# v0.3.1 - 2021-04-03
* Replace unsafe code with `pin-project` and `tokio::pin!()`.

# v0.3.0 - 2021-03-02
* Publicly re-export the `hyper` crate.
* Upgrade to `tokio-tungstenite` 0.14 and `tungstenite` 0.13.

# v0.2.1 - 2021-02-12
* Inspect all `Connection` and `Upgrade` headers in `is_upgrade_request()`.
* Inspect all comma separated values in `Connection` headers in `is_upgrade_request()` (this was already done for `Upgrade` headers).

# v0.2.0 - 2021-02-06
* Rename `upgrade_requested` to `is_upgrade_request`.

# v0.1.1 - 2021-02-06
* Fix category slug in Cargo manifest.

# v0.1.0 - 2021-02-06
* Initial release.
