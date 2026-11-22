# Playground

Static browser harness for `@cyg/testing`.

```bash
cd playground
python3 -m http.server 8080
# open http://127.0.0.1:8080
```

It uses the same deterministic `TestRoom` as the CI tests, so a state you see
here can be copied into a test with the same seed.
