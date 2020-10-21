FROM alpine:edge

RUN apk add git gcc cargo musl musl-dev pkgconfig openssl openssl-dev
COPY . /src
RUN cd /src && /usr/bin/cargo install --path .
RUN mv /root/.cargo/bin/ws-proxy /usr/bin/ws-proxy

RUN apk del git curl gcc cargo musl-dev pkgconfig openssl-dev
RUN apk add libgcc

ENTRYPOINT ["ws-proxy"]
