# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
import sys
import os
import pytest
from fastapi.testclient import TestClient

# Make sure we can import the module
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from tool_proxy import app

client = TestClient(app)

def test_module_leak():
    """Test that repeatedly executing a tool from the same file doesn't cause unbounded growth in sys.modules."""
    import tempfile
    
    with tempfile.NamedTemporaryFile(suffix=".py", delete=False, mode='w') as f:
        f.write("def dummy_tool(args):\n    return args.get('val', 0)\n")
        temp_path = f.name
        
    try:
        # Run once to initialize and get baseline
        response = client.post("/exec", json={
            "interpreter": "python",
            "script_path": temp_path,
            "action": "dummy_tool",
            "args": {"val": 1}
        })
        assert response.status_code == 200
        
        baseline_modules_count = len(sys.modules)
        
        # Run multiple times
        for i in range(10):
            response = client.post("/exec", json={
                "interpreter": "python",
                "script_path": temp_path,
                "action": "dummy_tool",
                "args": {"val": i}
            })
            assert response.status_code == 200
            
        final_modules_count = len(sys.modules)
        
        # Assert that sys.modules hasn't grown (unbounded leak is fixed)
        assert final_modules_count == baseline_modules_count, \
            f"Module leak detected: sys.modules grew from {baseline_modules_count} to {final_modules_count}"
            
    finally:
        if os.path.exists(temp_path):
            os.unlink(temp_path)
