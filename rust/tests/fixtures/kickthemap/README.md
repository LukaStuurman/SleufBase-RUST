# Synthetic KickTheMap protocol control

`storage-control.json` contains only public AWS example credentials and a synthetic session token. It contains no real account, production storage location or captured service data.

The encrypted envelope was checked against the pure decryption methods from original SleufBase main `0257f1d6bd10f90741bda2bbeff9522c210554bc`. Its signed URL was independently produced by botocore 1.43.120 `S3SigV4QueryAuth` (PyCryptodome 4.2.0 for the control envelope), including UTF-8 paths, query escaping and a temporary session token. Rust compares the decoded envelope and every signed query parameter. A second Rust test uses AWS's published Signature V4 example.

Recheck with `tools/verify_kickthemap_protocol_control.py --original /path/to/SleufBase/kickthemap.py` in a development environment containing pycryptodome and boto3. The control performs no network or account I/O.

Reference: https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html
