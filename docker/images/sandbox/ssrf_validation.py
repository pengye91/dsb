# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2025-2026 Tom Xie
import ipaddress
import socket
from urllib.parse import urlparse

class SSRFValidationError(Exception):
    pass

def validate_url(url: str) -> str:
    """
    Validates a URL against SSRF attacks.
    Checks scheme, resolves hostname, and blocks private/loopback IP addresses
    regardless of input format.
    
    Args:
        url: The URL string to validate
        
    Returns:
        The original URL if validation passes
        
    Raises:
        SSRFValidationError: If validation fails
    """
    if not url:
        raise SSRFValidationError("URL cannot be empty")
        
    if not url.startswith(('http://', 'https://')):
        raise SSRFValidationError("Invalid URL: must start with http:// or https://")
        
    try:
        parsed = urlparse(url)
    except Exception as e:
        raise SSRFValidationError(f"Failed to parse URL: {str(e)}")
        
    hostname = parsed.hostname
    if not hostname:
        raise SSRFValidationError("Invalid URL: missing hostname")
        
    try:
        # Resolve hostname to IPs to catch DNS-based SSRF and canonicalize IPs
        addr_info = socket.getaddrinfo(hostname, None)
    except socket.gaierror as e:
        raise SSRFValidationError(f"Could not resolve hostname: {str(e)}")
        
    for res in addr_info:
        ip_str = res[4][0]
        try:
            ip = ipaddress.ip_address(ip_str)
            if ip.is_private:
                raise SSRFValidationError(f"URL points to a private IP address: {ip_str}")
            if ip.is_loopback:
                raise SSRFValidationError(f"URL points to a loopback IP address: {ip_str}")
            if ip.is_link_local:
                raise SSRFValidationError(f"URL points to a link-local IP address: {ip_str}")
            # Also block unspecified/multicast just in case
            if ip.is_unspecified or ip.is_multicast or ip.is_reserved:
                raise SSRFValidationError(f"URL points to a restricted IP address type: {ip_str}")
        except ValueError:
            # Should not happen if getaddrinfo returns it, but be safe
            pass
            
    # For mac OS / certain python versions, getaddrinfo on octal doesn't correctly parse
    # Let's also do a direct check just to be safe if the hostname is an IP string
    try:
        # Check if the hostname itself can be parsed as an IP directly
        # ipaddress module handles standard formats
        ip_obj = ipaddress.ip_address(hostname)
        if ip_obj.is_private or ip_obj.is_loopback or ip_obj.is_link_local or ip_obj.is_unspecified or ip_obj.is_multicast or ip_obj.is_reserved:
            raise SSRFValidationError(f"URL points to a restricted IP address type: {hostname}")
    except ValueError:
        pass
        
    # Some octal bypasses like 0177.0.0.1 might evaluate to 177.0.0.1 in getaddrinfo, 
    # but the browser could treat it as octal 127.0.0.1. So we explicitly check for leading zeroes.
    if hostname.replace('.', '').isdigit():
        parts = hostname.split('.')
        for part in parts:
            if len(part) > 1 and part.startswith('0') and not part.startswith('0x'):
                # This could be an octal representation
                try:
                    # Convert octal parts to decimal and check
                    dec_parts = [str(int(p, 8)) if p.startswith('0') and len(p)>1 and not p.startswith('0x') else p for p in parts]
                    dec_ip = '.'.join(dec_parts)
                    try:
                        ip_obj = ipaddress.ip_address(dec_ip)
                        if ip_obj.is_private or ip_obj.is_loopback or ip_obj.is_link_local or ip_obj.is_unspecified or ip_obj.is_multicast or ip_obj.is_reserved:
                            raise SSRFValidationError(f"URL points to a restricted IP address type (octal): {hostname}")
                    except ValueError:
                        pass
                except ValueError:
                    pass
            
    return url
