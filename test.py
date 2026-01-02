import requests
from uuid import UUID

url = "http://127.0.0.1:3000"
port = 80
addr = "www.baidu.com"

data = (
    bytes([0])
    + UUID("891e542a-7054-44f4-9149-ef89d00a7387").bytes
    + bytes([0, 1])
    + port.to_bytes(2, "big")
    + bytes([0])
    + len(addr).to_bytes(1, "big")
    + addr.encode("utf-8")
    + b"GET / HTTP/1.1\r\nHost: www.baidu.com\r\n\r\n"
)

req = requests.post(url, data=data)

print(req.content[:100])