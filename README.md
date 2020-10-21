WebSocket Proxy
===============
Can be used for debugging purposes or for forwarding WebSocket traffic to another machine, e.g. for resolving issues with CORS.

```
cargo install --git https://github.com/kirillt/ws-proxy
ws-proxy ws://127.0.0.1:9944 1337 --pretty-jsons
# connect your client to 1337 port instead of 9944
tail -f ws-proxy.{client,server}.log
```
