#!/bin/bash

echo "╔════════════════════════════════════════════════╗"
echo "║         SYSTEM PERFORMANCE BENCHMARK           ║"
echo "╚════════════════════════════════════════════════╝"
echo ""

echo "System Info:"
echo "════════════════════════════════════════════════"
uname -a
echo ""
cat /proc/cpuinfo | grep "model name" | head -1
echo "CPU Count: $(nproc)"
echo "Memory: $(free -h | grep Mem | awk '{print $2}')"
echo ""

echo "CPU Isolation:"
echo "════════════════════════════════════════════════"
if [ -f /sys/devices/system/cpu/isolated ]; then
    echo "Isolated CPUs: $(cat /sys/devices/system/cpu/isolated)"
else
    echo "  No CPUs isolated"
fi
echo ""

echo "CPU Governor:"
echo "════════════════════════════════════════════════"
cat /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor | sort | uniq -c
echo ""

echo "RT Kernel:"
echo "════════════════════════════════════════════════"
if uname -a | grep -q PREEMPT; then
    echo "RT kernel detected"
else
    echo "Standard kernel (not RT)"
fi
echo ""

echo "Running throughput test..."
echo "════════════════════════════════════════════════"
cargo run --release --example throughput_test 2>&1 | tail -20
echo ""

echo "Running latency benchmark..."
echo "════════════════════════════════════════════════"
cargo bench --bench latency_bench -- --sample-size 10 2>&1 | grep "time:"