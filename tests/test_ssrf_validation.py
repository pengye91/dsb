import pytest
import sys
import os

# Add the docker/images/sandbox directory to sys.path to import ssrf_validation
sys.path.append(os.path.join(os.path.dirname(os.path.dirname(__file__)), 'docker/images/sandbox'))

from ssrf_validation import validate_url, SSRFValidationError

class TestSSRFValidation:
    def test_allowed_urls(self):
        assert validate_url("http://example.com") == "http://example.com"
        assert validate_url("https://google.com") == "https://google.com"
        
    def test_blocked_loopback(self):
        with pytest.raises(SSRFValidationError):
            validate_url("http://127.0.0.1")
            
        with pytest.raises(SSRFValidationError):
            validate_url("http://localhost")
            
        with pytest.raises(SSRFValidationError):
            validate_url("http://[::1]")
            
    def test_blocked_private(self):
        with pytest.raises(SSRFValidationError):
            validate_url("http://10.1.2.3")
            
        with pytest.raises(SSRFValidationError):
            validate_url("http://192.168.1.1")
            
        with pytest.raises(SSRFValidationError):
            validate_url("http://172.16.0.1")
            
    def test_blocked_non_http_schemes(self):
        with pytest.raises(SSRFValidationError):
            validate_url("file:///etc/passwd")
            
        with pytest.raises(SSRFValidationError):
            validate_url("gopher://127.0.0.1")
            
        with pytest.raises(SSRFValidationError):
            validate_url("ftp://example.com")
            
        with pytest.raises(SSRFValidationError):
            validate_url("dict://example.com")
            
    def test_blocked_octal_hex_ip_formats(self):
        with pytest.raises(SSRFValidationError):
            validate_url("http://0177.0.0.1")
            
        with pytest.raises(SSRFValidationError):
            validate_url("http://0x7f.0.0.1")
            
        with pytest.raises(SSRFValidationError):
            # Decimal for 127.0.0.1
            validate_url("http://2130706433")
            
        with pytest.raises(SSRFValidationError):
            # Hex for 10.0.0.1
            validate_url("http://0x0a000001")
