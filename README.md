# yew-webtransport

Placeholder for Yew WebTransport library that will be used to replace 
the webtransports in https://github.com/security-union/rust-zoom


Run Chrome on Mac OS with:
```
./Google\ Chrome.app/Contents/MacOS/Google\ Chrome \
  --enable-experimental-web-platform-features \
  --ignore-certificate-errors-spki-list=wexvmLCxw/XeTkZF06w6hqujOmN/6+0TC23sSBSnHFg= \
  --origin-to-force-quic-on=127.0.0.1:4433 \
  --user-data-dir=.\quic-userdata \
  https://webtransport.day
```