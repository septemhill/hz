#!/bin/bash

# Test runner for Lang examples
# This script runs all test files and verifies expected behavior

cd /Users/septemlee/private/lang

echo "========================================="
echo "Lang Language Test Runner"
echo "========================================="
echo ""

# Expected to succeed (correct syntax) - these must have explicit io import
success_tests=(
    "test_tuple.lang"
    "test_import_group.lang"
    # "test_features.lang"
    # "test_optional_simple.lang"
    # "test_optional_value.lang"
    # "test_return_simple.lang"
    # "test_return_simple2.lang"
    # "test_simple_ret.lang"
    # "test_import.lang"
    # "test_tuple_ret.lang"
    # "test_tuple6.lang"
    # "test_var_reassign.lang"
)

# Expected to fail (error cases)
error_tests=(
    # "test_without_import.lang"                    # No explicit import for io
    # "test_duplicate_import.lang"
    # "test_import_error.lang"
    # "test_multiple_imports.lang"
    # "test_same_package_different_alias.lang"
    # "test_const_reassign_error.lang"
    # "test_var_no_init2.lang"
    # "test_var_no_init3.lang"
    # "test_method.lang"
    # "test_method_simple.lang"
    # "test_interface.lang"
    # "test_math.lang"
    # "test_tuple4.lang"
    # "test_var_no_type.lang"
    # "test_var_const.lang"
)

passed=0
failed=0
total=0

# Test success cases
echo "=== Testing Success Cases ==="
echo ""
for file in "${success_tests[@]}"; do
    total=$((total + 1))
    
    echo "-----------------------------------------"
    echo "Testing: $file (expected: success)"
    echo "-----------------------------------------"
    
    # Run the test and capture output
    output=$(cargo run -- run "examples/$file" 2>&1)
    
    # Check for actual Lang error
    if echo "$output" | grep -q "^error"; then
        actual="error"
    elif echo "$output" | grep -qE "^(\s*)Error:|^(\s*)-->.*error"; then
        actual="error"
    elif echo "$output" | grep -q "Generated LLVM IR"; then
        actual="success"
    else
        actual="success"
    fi
    
    # Compare expected vs actual
    if [ "$actual" == "success" ]; then
        echo "✓ PASSED (expected success, got success)"
        passed=$((passed + 1))
    else
        echo "✗ FAILED (expected success, got error)"
        # Show actual error message
        error_msg=$(echo "$output" | grep -E "^error|^Error" | head -3)
        if [ -n "$error_msg" ]; then
            echo "Error: $error_msg"
        fi
        failed=$((failed + 1))
    fi
    echo ""
done

# Test error cases
echo "=== Testing Error Cases ==="
echo ""
for file in "${error_tests[@]}"; do
    total=$((total + 1))
    
    echo "-----------------------------------------"
    echo "Testing: $file (expected: error)"
    echo "-----------------------------------------"
    
    # Run the test and capture output
    output=$(cargo run -- run "examples/$file" 2>&1)
    
    # Check for actual Lang error
    if echo "$output" | grep -q "^error"; then
        actual="error"
    elif echo "$output" | grep -qE "^(\s*)Error:|^(\s*)-->.*error"; then
        actual="error"
    elif echo "$output" | grep -q "Generated LLVM IR"; then
        actual="success"
    else
        actual="error"
    fi
    
    # Compare expected vs actual
    if [ "$actual" == "error" ]; then
        echo "✓ PASSED (expected error, got error)"
        passed=$((passed + 1))
    else
        echo "✗ FAILED (expected error, got success)"
        failed=$((failed + 1))
    fi
    echo ""
done

echo "========================================="
echo "Test Results"
echo "========================================="
echo "Total: $total"
echo "Passed: $passed"
echo "Failed: $failed"
echo ""

if [ $failed -gt 0 ]; then
    exit 1
else
    exit 0
fi
