#!/usr/bin/env python3
import os, sys
method = os.environ.get('REQUEST_METHOD', '')
query  = os.environ.get('QUERY_STRING', '')
length = int(os.environ.get('CONTENT_LENGTH', '0'))
body   = sys.stdin.read(length) if length > 0 else ''
print('Content-Type: text/plain')
print()
print(f'ok method={method} query={query} body_len={len(body)}')
