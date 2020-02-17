```
cargo install --git https://github.com/kirillt/ws-debug
ws-debug ws://127.0.0.1:9944 1337 --pretty-jsons
# connect your client to 1337 port instead of 9944
tail -f ws-debug.{client,server}.log
```
