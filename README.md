# Easycheck

![License](https://img.shields.io/github/license/easybill/easycheck)
![Build status](https://img.shields.io/github/actions/workflow/status/easybill/easycheck/ci.yml)
![Release version](https://img.shields.io/github/v/release/easybill/easycheck)

Easycheck is a lightweight HTTP server designed to monitor and report the status of the server it operates on. It
achieves this by offering a single endpoint that users can query to check the server’s health.

### How does it work?

1. Periodic Checks: Easycheck performs regular checks on specific services, such as an HTTP endpoint or a TCP socket,
   running on the host system.
2. Determining Availability: If any of these checks fail, Easycheck marks the server as "unavailable." Consequently,
   when the endpoint is queried, the server responds with a status code "503" (Service Unavailable).
3. Successful Checks: If all checks pass, the server is considered healthy, and the endpoint returns the status code
   "200" (OK).
4. Maintenance Override: You can manually override the checks by creating a special maintenance file. When this file is
   present, the server will always be reported as "unavailable," regardless of the actual check results.

This setup ensures that the server is monitored and can be marked as unavailable in case a backend service is no longer
responding, for example, in a load balancer.

### Configuration

Easycheck can be configured with command line parameters or environment variables.

| Command Line Option         | Environment Variable                | Required | Default              | Description                                                                                                                                                                                                                                 |
|-----------------------------|-------------------------------------|----------|----------------------|---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| `--bind`                    | `EASYCHECK_BIND_HOST`               | Yes      |                      | Sets the bind host for the HTTP endpoint. Format: `ip:port` (or for ipv6 addresses: `[ip]:port`                                                                                                                                             |
| `--revalidation-interval`   | `EASYCHECK_REVALIDATE_INTERVAL`     | No       | 5                    | The interval between check executions in seconds. Must be positive.                                                                                                                                                                         |
| `--force-success-file-path` | `EASYCHECK_FORCE_SUCCESS_FILE_PATH` | No       | `easycheck.success`  | Defines the path where the force-success file is located. If this file exists the service is marked as available even if some checks failed.                                                                                                |
| `--mtc-file-path`           | `EASYCHECK_MTC_FILE_PATH`           | No       | `easycheck.disabled` | Defines the path where the maintenance file is located. Can be an absolute or relative path.                                                                                                                                                |
| `--socket-addr`             | `EASYCHECK_SOCKET_ADDR`             | No       |                      | Defines the socket address to check regularly if still responding. Easycheck connects to the socket, sends a `QUIT` message and tries to receive a response. The check if successful if the connection succeeds and a response is received. |
| `--http-url`                | `EASYCHECK_HTTP_URL`                | No       |                      | Defines the http address to check regularly. Further configuration can be done with the other http options. Format: `http[s]://<host>:[port]/[path]`                                                                                        |
| `--http-method`             | `EASYCHECK_HTTP_METHOD`             | No       |                      | Defines the http method to use for executing the http status check. Only has effect if an http url is given to check.                                                                                                                       |
| `--http-status-codes`       | `EASYCHECK_HTTP_STATUS_CODES`       | No       |                      | Defines the numerical http status codes that should be considered as a successful check.                                                                                                                                                    |

### Compile from source

1. Clone this repository
2. If you're on Linux you might need to install `build-essentials`
3. Make sure you have [Cargo installed](https://doc.rust-lang.org/cargo/getting-started/installation.html) and
   run `cargo build --release`
4. Take the final file from `target/release/easycheck[.extension]`

### Download pre-compiled binaries

The binaries for easycheck are pre-compiled available attached to each
[release](https://github.com/easybill/easycheck/releases). These are currently pre-compiled for
the following targets:

| Target | Architectures     |
|--------|-------------------|
| Apple  | x64, x86, aarch64 |
| Linux  | x64, x86, aarch64 |
