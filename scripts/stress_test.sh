#!/bin/bash

# ── Config ────────────────────────────────────────────────────────────────────
HOST="127.0.0.1"
PORT="8080"
BASE="http://${HOST}:${PORT}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

PASS=0
FAIL=0

# ── Helpers ───────────────────────────────────────────────────────────────────

log()     { echo -e "${BLUE}[INFO]${NC} $1"; }
pass()    { echo -e "${GREEN}[PASS]${NC} $1"; PASS=$((PASS + 1)); }
fail()    { echo -e "${RED}[FAIL]${NC} $1"; FAIL=$((FAIL + 1)); }
section() { echo -e "\n${YELLOW}══════════════════════════════════════${NC}";
            echo -e "${YELLOW} $1${NC}";
            echo -e "${YELLOW}══════════════════════════════════════${NC}"; }

# Sends a GET request and returns the HTTP status code
get_status() {
    curl -s -o /dev/null -w "%{http_code}" "$1"
}

# ── Setup ─────────────────────────────────────────────────────────────────────

section "Setup"

mkdir -p www/stress_test
echo "<html><body><h1>Stress Test</h1></body></html>" > www/stress_test/index.html
python3 -c "print('x' * 1024)"   > www/stress_test/1kb.txt
python3 -c "print('x' * 102400)" > www/stress_test/100kb.txt
log "Test files created"

# ── Phase 1: Server is alive ──────────────────────────────────────────────────

section "Phase 1: Server health check"

# What this tests:
# The most basic check — can the server respond at all?
# We send one request to each of our test files and verify
# we get 200 back. If any of these fail, nothing else matters.

STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "GET /stress_test/index.html → 200"
else
    fail "GET /stress_test/index.html → expected 200, got $STATUS"
fi

STATUS=$(get_status "$BASE/stress_test/1kb.txt")
if [ "$STATUS" = "200" ]; then
    pass "GET /stress_test/1kb.txt → 200"
else
    fail "GET /stress_test/1kb.txt → expected 200, got $STATUS"
fi

STATUS=$(get_status "$BASE/stress_test/100kb.txt")
if [ "$STATUS" = "200" ]; then
    pass "GET /stress_test/100kb.txt → 200"
else
    fail "GET /stress_test/100kb.txt → expected 200, got $STATUS"
fi

STATUS=$(get_status "$BASE/does_not_exist.html")
if [ "$STATUS" = "404" ]; then
    pass "GET /does_not_exist.html → 404"
else
    fail "GET /does_not_exist.html → expected 404, got $STATUS"
fi

STATUS=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH "$BASE/stress_test/index.html")
if [ "$STATUS" = "405" ]; then
    pass "PATCH /stress_test/index.html → 405"
else
    fail "PATCH /stress_test/index.html → expected 405, got $STATUS"
fi

# ── Final report ──────────────────────────────────────────────────────────────

section "Report"
echo -e "Passed: ${GREEN}${PASS}${NC}"
echo -e "Failed: ${RED}${FAIL}${NC}"


# ── Phase 2: Concurrent requests ─────────────────────────────────────────────

section "Phase 2: Concurrent requests"

# What this tests:
# We fire N requests at the same time using & (background processes)
# then wait for all of them to finish.
# This directly tests your epoll event loop — if it has bugs,
# some requests will fail or return wrong status codes.

# Helper — fires N concurrent GETs, returns how many got expected status
concurrent_get() {
    local url="$1"
    local count="$2"
    local expected="$3"

    local results=""
    for i in $(seq 1 $count); do
        results+=$(curl -s -o /dev/null -w "%{http_code}\n" "$url")$'\n' &
    done
    wait

    # rerun cleanly to actually capture results
    results=""
    for i in $(seq 1 $count); do
        results+="$(curl -s -o /dev/null -w "%{http_code}\n" "$url") "&
    done
    wait

    local pids=()
    local tmpfile=$(mktemp)
    for i in $(seq 1 $count); do
        curl -s -o /dev/null -w "%{http_code}\n" "$url" >> "$tmpfile" &
        pids+=($!)
    done
    for pid in "${pids[@]}"; do wait "$pid"; done

    local got=$(grep -c "^${expected}$" "$tmpfile")
    rm -f "$tmpfile"
    echo "$got"
}

# 10 concurrent GETs — warmup
log "10 concurrent GETs → index.html"
GOT=$(concurrent_get "$BASE/stress_test/index.html" 10 "200")
if [ "$GOT" -eq 10 ]; then
    pass "10 concurrent GETs — all 200 ($GOT/10)"
else
    fail "10 concurrent GETs — only $GOT/10 succeeded"
fi

# 50 concurrent GETs
log "50 concurrent GETs → index.html"
GOT=$(concurrent_get "$BASE/stress_test/index.html" 50 "200")
if [ "$GOT" -eq 50 ]; then
    pass "50 concurrent GETs — all 200 ($GOT/50)"
else
    fail "50 concurrent GETs — only $GOT/50 succeeded"
fi

# 100 concurrent GETs — real pressure
log "100 concurrent GETs → index.html"
GOT=$(concurrent_get "$BASE/stress_test/index.html" 100 "200")
if [ "$GOT" -eq 100 ]; then
    pass "100 concurrent GETs — all 200 ($GOT/100)"
else
    fail "100 concurrent GETs — only $GOT/100 succeeded"
fi

# 50 concurrent GETs of 100KB file — tests larger responses
log "50 concurrent GETs → 100kb.txt"
GOT=$(concurrent_get "$BASE/stress_test/100kb.txt" 50 "200")
if [ "$GOT" -eq 50 ]; then
    pass "50 concurrent GETs (100KB) — all 200 ($GOT/50)"
else
    fail "50 concurrent GETs (100KB) — only $GOT/50 succeeded"
fi

# 50 concurrent 404s — error handling under concurrency
log "50 concurrent GETs → missing file (expect 404)"
GOT=$(concurrent_get "$BASE/does_not_exist.html" 50 "404")
if [ "$GOT" -eq 50 ]; then
    pass "50 concurrent 404s — all correct ($GOT/50)"
else
    fail "50 concurrent 404s — only $GOT/50 got 404"
fi

# Server still alive after all that?
STATUS=$(get_status "$BASE/stress_test/index.html")
if [ "$STATUS" = "200" ]; then
    pass "Server still alive after concurrent load"
else
    fail "Server not responding after concurrent load (got $STATUS)"
fi