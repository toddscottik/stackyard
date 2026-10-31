# S0-WIN-SERVERS evidence

Date: 31 October 2026.

Target: Windows 10 Pro 22H2, build 19045.6456, x64 (`AMD64`). No macOS run was performed.

Passing command, from `D:\_work\laragon_alternative`, Windows PowerShell 5.1:

```text
powershell.exe -NoProfile -ExecutionPolicy Bypass -File D:\_work\laragon_alternative\scripts\verification\windows\servers.ps1 -Action Verify
```

Exit code 0. Started 2026-10-31T23:50:49+03:00, finished 2026-10-31T23:51:48+03:00. Structured results: `.artifacts/S0-WIN-SERVERS/results.json`. The default action is Help and does not download.

All listeners were bound to `127.0.0.1` on high ports. No process was registered as a Windows service. Pre-existing services stayed unchanged: `Apache2.4` Stopped/Manual and `mysql` Stopped/Manual, before and after. No PostgreSQL Windows service was present. The process `PATH` was unchanged. Cleanup stopped only processes whose command line contained this task's artifact root.

## Ports

| Role | Address |
| --- | --- |
| Apache | 127.0.0.1:18080 |
| Proxy upstream (script-owned TCP listener, not Node) | 127.0.0.1:18081 |
| MySQL | 127.0.0.1:13306 |
| PostgreSQL | 127.0.0.1:15432 |
| Occupied-port demonstration | 127.0.0.1:18099 |

## Apache

Listing page fetched at run time: [https://www.apachelounge.com/download/](https://www.apachelounge.com/download/). It listed the Win64 VS18 zip below (still 2.4.69) and states that the build runs on Windows 10. It also states the prerequisite: Visual C++ Redistributable 14.51.36247 for Visual Studio 2017-2026 x64 (`https://aka.ms/vc14/vc_redist.x64.exe`).

| Item | Value |
| --- | --- |
| File | `httpd-2.4.69-261002-Win64-VS18.zip` |
| URL | `https://www.apachelounge.com/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip` |
| Digest source | `https://www.apachelounge.com/download/VS18/binaries/httpd-2.4.69-261002-Win64-VS18.zip.txt` |
| SHA256 | `77D715D0D9CEE731E2251E7DDC2090A7A8E6D41FEDEF2E9827CD918593C5A652` |

The SHA256 was checked before extract. A PGP signature is also linked from the same page (`httpd-2.4.69-261002-Win64-VS18.zip.asc`, key `Steffen_Land.asc`); this probe verified the published SHA256 file and did not check the signature.

Registry `HKLM:\SOFTWARE\Microsoft\VisualStudio\14.0\VC\Runtimes\x64` reported `v14.51.36247.00`, installed. That meets the page's 14.51.36247 requirement. The redistributable was not installed by this probe.

Binary, under a path that contains a space: `.artifacts/S0-WIN-SERVERS/space path/apache/Apache24/bin/httpd.exe`.

Generated config: `.artifacts/S0-WIN-SERVERS/config/httpd.conf` (`httpd -t` reported Syntax OK). The stock `httpd.conf` inside the zip was not edited, and no system Apache config was edited. `httpd -M` showed `proxy_module` and `proxy_http_module`.

Two name-based virtual hosts on `127.0.0.1:18080`:

- `Host: site-a.test` `GET /secret-a.txt` returned 200 `site-a-only`.
- `Host: site-b.test` `GET /secret-a.txt` returned 404 and did not return the site-a body.
- `Host: site-b.test` `GET /other.txt` returned 200 `site-b-only`.
- `Host: site-a.test` `GET /upstream/` returned 200 `upstream-marker-S0-WIN-SERVERS` from the script-owned listener on `127.0.0.1:18081`.
- `Host: site-b.test` `GET /upstream/` returned 404 and did not return the upstream body.

The server was stopped and started again; the site-a fixture and the proxy check succeeded after restart. After the final stop, port 18080 was free, the httpd process tree was gone, and the upstream listener and port 18081 were gone. Nothing this probe started was left listening on port 80 or 443.

## MySQL

Official CDN object. `https://dev.mysql.com/downloads/mysql/8.4.html` returned HTTP 403 to this client. `mysql-8.4.14` and newer Win64 zip MD5 sidecars returned HTTP 404. `8.4.13` is the newest 8.4 Windows x64 zip whose publisher MD5 sidecar responded HTTP 200. A `.sha256` sidecar returned HTTP 404, so verification used the published MD5.

| Item | Value |
| --- | --- |
| File | `mysql-8.4.13-winx64.zip` |
| URL | `https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip` |
| Digest source | `https://cdn.mysql.com/Downloads/MySQL-8.4/mysql-8.4.13-winx64.zip.md5` |
| MD5 | `7b94489d5524dea9b27097068bea25a8` |

The MD5 was checked before extract. Binary, under a path that contains a space: `.artifacts/S0-WIN-SERVERS/space path/mysql/mysql-8.4.13-winx64/bin/mysqld.exe`.

Server options came from a generated `.artifacts/S0-WIN-SERVERS/config/my.ini` passed as the first `--defaults-file`. `bind-address=127.0.0.1`, port 13306, `mysqlx=0`. No system `my.ini` was read or edited. The data directory is `.artifacts/S0-WIN-SERVERS/data/mysql`, created empty earlier in this same task by `mysqld --no-defaults --initialize-insecure`. The passing Verify run reused that directory. It did not point at any external MySQL data directory.

Nonempty refusal, both exit code 1, with `MY-010457 --initialize specified but the data directory has files in it. Aborting.`:

- `.artifacts/S0-WIN-SERVERS/data/mysql-nonempty` (a directory that only contained a marker file).
- `.artifacts/S0-WIN-SERVERS/data/mysql` after it had already been initialized.

Disposable credentials are in `.artifacts/S0-WIN-SERVERS/secrets/mysql-password.txt` and the client defaults file next to it. They are not copied here.

Query before and after restart (`probe`, `@@version`, `@@port`, `@@bind_address`):

```text
1    8.4.13    13306    127.0.0.1
```

Shutdown used `mysqladmin` with `--defaults-file` (no password on the command line). After the final shutdown the port was free and the `mysqld` process was gone. The Windows `mysql` service stayed Stopped.

## PostgreSQL

The binaries page [https://www.enterprisedb.com/download-postgresql-binaries](https://www.enterprisedb.com/download-postgresql-binaries) listed Version 18.6, Windows x86-64, as `https://sbp.enterprisedb.com/getfile.jsp?fileid=1260609`. That URL redirected to the zip below. The SHA256 sidecar next to the zip returned HTTP 200 and was checked before extract.

| Item | Value |
| --- | --- |
| File | `postgresql-18.6-5-windows-x64-binaries.zip` |
| URL | `https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip` |
| Digest source | `https://get.enterprisedb.com/postgresql/postgresql-18.6-5-windows-x64-binaries.zip.sha256` |
| SHA256 | `e2246ba91d22345bc3d017586c09ede52d9df180b1eeb480f050445f1cad84e2` |

[postgresql.org Windows packages](https://www.postgresql.org/download/windows/) lists Postgres 18 as tested on Windows Server 2025 and 2022, not Windows 10. It says desktop releases can generally be expected to run. This probe still launched it. The server log recorded `PostgreSQL 18.6 on x86_64-windows, compiled by msvc-19.44.35229`.

Binary, under a path that contains a space: `.artifacts/S0-WIN-SERVERS/space path/postgresql/pgsql/bin/initdb.exe` (same `bin` directory for `pg_ctl.exe` and `psql.exe`). `pg_ctl register` was not used.

`initdb` created `.artifacts/S0-WIN-SERVERS/data/postgres` earlier in this task (`--no-locale --encoding=UTF8`, SCRAM, password file under the artifact `secrets` directory). The passing Verify run reused that cluster. `listen_addresses = '127.0.0.1'` and `port = 15432` are in that data directory's `postgresql.conf`. `pg_hba.conf` allows only `127.0.0.1/32` with `scram-sha-256`.

Nonempty refusal: `initdb` against `.artifacts/S0-WIN-SERVERS/data/postgres-nonempty` exited 1 with `directory ... exists but is not empty` and did not create `PG_VERSION`.

Query before and after `pg_ctl restart`, then `pg_ctl stop -m fast`:

```text
probe | port  | addr
1     | 15432 | 127.0.0.1
```

After stop, port 15432 was free and the `postgres.exe` processes were gone. The password file is `.artifacts/S0-WIN-SERVERS/secrets/postgres-password.txt`.

## Occupied port

The script bound its own listener to `127.0.0.1:18099` (pid 87968), then tried to bind that port again. The second bind failed. The script reported `127.0.0.1:18099 is already in use. Refusing to take it. The existing listener was left running.` The occupant was still running and the port still accepted connections. Cleanup later stopped that listener because this script had started it. It was not stopped as part of the failure path.

## Cleanup

Final cleanup found the Apache, MySQL, and PostgreSQL processes already exited. It stopped the occupied-port listener (alive before cleanup, gone after). Archives, extracts, data directories, logs, configs, and secret files were kept under `.artifacts/S0-WIN-SERVERS/`.

## Limitations

- macOS was not run. PostgreSQL 18 is not an explicit Windows 10 certification on postgresql.org; the launch above is the observed result on this Windows 10 machine.
- MySQL integrity used the publisher MD5 sidecar. A publisher SHA256 sidecar was not available (HTTP 404). The MySQL HTML download page returned HTTP 403 here.
- Apache PGP was not verified; the published SHA256 checksum file was.
- The passing Verify invocation reused MySQL and PostgreSQL data directories created earlier the same evening by this script. The same invocation still refused a nonempty data directory for both engines.
- The Visual C++ redistributable was already installed. A missing runtime would block Apache launch; this probe does not install it.
- No Windows service install, hosts-file change, trust-store change, or global PATH change was made.
