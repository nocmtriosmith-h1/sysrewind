# Security policy

Sysrewind installs a signed kernel driver (PawnIO) and runs a small helper as SYSTEM, so security reports are taken seriously.

## Reporting a vulnerability

Please **do not open a public issue** for security problems.
Use GitHub's private reporting instead: **Security → Report a vulnerability** on this repository.

Include what you found, how to reproduce it, and the Sysrewind version. You should get a reply within 7 days.

## What runs with elevated rights

| Component | Runs as | What it does |
|---|---|---|
| Installer (`Sysrewind_x.y.z_x64-setup.exe`) | Administrator, once | Installs to Program Files, installs PawnIO if missing, registers the **Sysrewind Sensors** task |
| `sysrewind.exe --sensors` | SYSTEM, at boot | Reads CPU temperature through PawnIO (read-only MSR/SMN registers) and disk temperature through SMART; writes `%ProgramData%\Sysrewind\sensors.json` |
| Sysrewind app and window | The signed-in user, **not** elevated | Records metrics, shows the UI |

Hardening in place:

- The SYSTEM task is only registered when Sysrewind is installed under `Program Files`, which ordinary users cannot modify.
- `%ProgramData%\Sysrewind` has inheritance removed: SYSTEM and Administrators have full control, Users can only read.
- `PawnIOLib.dll` is loaded by absolute path, and its dependencies are resolved only from its own folder and System32.
- The helper only **reads** hardware registers. It never writes MSRs.
- The UI uses a strict Content Security Policy, and every process name is escaped before it is shown.
- No network access, accounts or telemetry. All data stays in `%APPDATA%\com.sysrewind.app`.

## Supported versions

Only the latest release receives security fixes.
