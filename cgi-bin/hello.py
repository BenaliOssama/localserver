#!/usr/bin/env python3
import os
import sys

# Read environment variables the server will set
method  = os.environ.get("REQUEST_METHOD", "GET")
path    = os.environ.get("PATH_INFO", "/")
query   = os.environ.get("QUERY_STRING", "")

# Read body from stdin if POST
body = ""
if method == "POST":
    length = int(os.environ.get("CONTENT_LENGTH", "0"))
    if length > 0:
        body = sys.stdin.read(length)

# Write response to stdout — headers first, then blank line, then body
print("Content-Type: text/html")
print()  # blank line separates headers from body
print(f"""<!DOCTYPE html>
<html>
<body>
    <h1>Hello from CGI!</h1>
    <p>Method: {method}</p>
    <p>Path: {path}</p>
    <p>Query: {query}</p>
    <p>Body: {body}</p>
</body>
</html>""")