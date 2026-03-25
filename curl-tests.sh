# curl sends chunked automatically for streamed data
curl -i -X POST http://127.0.0.2:8080/uploads/chunked.txt \
     -H "Transfer-Encoding: chunked" \
     --data-binary "hello world"

# Read it back
curl http://127.0.0.2:8080/uploads/chunked.txt
