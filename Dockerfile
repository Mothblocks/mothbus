FROM rust:slim-buster as builder
WORKDIR /usr/src/mothbus
RUN apt-get update
RUN apt-get install -y curl
RUN curl -sL https://deb.nodesource.com/setup_18.x | bash -
RUN apt-get install -y nodejs
COPY . .
RUN npm install
RUN npm run-script build
RUN cargo install --path .

FROM alpine:latest AS certs
RUN apk --update add ca-certificates
RUN update-ca-certificates

FROM debian:buster-slim
WORKDIR /usr/bin/mothbus
COPY --from=builder /usr/local/cargo/bin/mothbus mothbus
COPY --from=builder /usr/src/mothbus/dist dist
COPY --from=builder /usr/src/mothbus/public public
COPY --from=certs /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/ca-certificates.crt
CMD ["./mothbus"]
