 Weber – a small web app to control services remotely

## Description

Weber is a small web app for running and controlling services on a remote Linux server. It was primarily developed to control a [Paper](https://papermc.io/) Minecraft server.

<!-- TODO: add a screenshot of the web UI -->
<!-- ![Weber screenshot](docs/screenshot.png) -->

## Features

<!-- TODO: list what the app can actually do, e.g.: -->
- Start, stop and restart services
- View service status and logs
- Password-protected admin login (Argon2 password hashing)

## Requirements

- Linux server

## Installation

```bash
git clone https://github.com/irudym/weber.git
cd weber
# build frontend at first 
cd frontend && npm ci && npm run build
# build the main app
cargo build --release
```

## Configuration

Set the following environment variables:

| Variable | Required | Description |
|---|---|---|
| `MONITOR_USER` | yes | Admin user name |
| `MONITOR_PASS_HASH` | yes | Argon2 password hash (see [Password hash generation](#password-hash-generation)) |
| `MONITOR_INSECURE_COOKIE` | no | If set, allows session cookies over plain HTTP. Use only for local testing. |

<!-- TODO: listen address/port, how services to control are configured -->

### Password hash generation

To generate an Argon2 password hash, run the program with the `hash-password` parameter:

```bash
./weber hash-password
```

Enter the password at the prompt, and the app will print the hash for you.

## Running

```bash
export MONITOR_USER=admin
export MONITOR_PASS_HASH='<hash from hash-password>'
./weber
```

For example, for test purposes the service may be started as following:
```bash
HASH=$(echo 'your-password' | /opt/weber/weber hash-password)

sudo -u minecraft env \
  MONITOR_USER=admin \
  MONITOR_PASS_HASH="$HASH" \
  MONITOR_INSECURE_COOKIE=1 \
  RUST_LOG=weber=debug,tower_http=info \
  /opt/weber/weber
```

Then open `http://127.0.0.1:3000` (or your server IP) in your browser.

## Running weber as a service
<!-- TODO: optional systemd unit example for running Weber as a service -->
To run the weber as a service, copy the application to opt/weber directory first (or any other, which is more convinient for you).
Create /etc/systemd/system/weber.service:
```
[Unit]
Description=weber process monitor
After=network-online.target
Wants=network-online.target

[Service]
# same user that owns the server files
User=minecraft                     
WorkingDirectory=/opt/weber
# MONITOR_USER, MONITOR_PASS_HASH, RUST_LOG
EnvironmentFile=/etc/weber.env     
ExecStart=/opt/weber/weber
Restart=on-failure
# only stop weber itself, not the processes it started
KillMode=process                   

[Install]
WantedBy=multi-user.target
```

/etc/weber.env holds the settings. Systemd doesn’t expand $ in values, so the hash goes in as it is, without quotes:
```
MONITOR_USER=admin
MONITOR_PASS_HASH=$argon2id$v=19$m=19456,t=2,p=1$...
RUST_LOG=weber=info
```

Then
```bash
sudo chmod 600 /etc/weber.env          # the hash shouldn't be world-readable
sudo systemctl daemon-reload
sudo systemctl enable --now weber
journalctl -u weber -f                 # follow the logs
```

## Development with hot reload
run 
```bash
npm run dev
````
in frontend/
then go to the main directory (cd..) and run
```bash
HASH=$(echo 'your-password' | cargo run hash-password)
MONITOR_USER=admin \
  MONITOR_PASS_HASH="$HASH" \
  MONITOR_INSECURE_COOKIE=1 \
  RUST_LOG=weber=debug,tower_http=info \
  cargo run
```


## Creating release builds
Run
```bash
npm run build
```
in frontend/ first, then 
```bash
cargo build --release
```
rust-embed takes whatever is in frontend/dist/ at that moment.


## Security notes

- Run Weber behind HTTPS (e.g. a reverse proxy such as nginx or Caddy) in production.
- Do not set `MONITOR_INSECURE_COOKIE` on a publicly reachable server.
- Wrap the password hash in single quotes when exporting it — Argon2 hashes contain `$` characters that the shell would otherwise expand.

## License
MIT
