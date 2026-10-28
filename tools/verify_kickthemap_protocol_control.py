"""Read-only original-Python and botocore control; synthetic fixture, no account/network.
Requires pycryptodome and boto3 in the development environment.
"""
import argparse,ast,base64,datetime,hashlib,json
from pathlib import Path
from unittest.mock import patch
from urllib.parse import quote,urlencode,urlsplit,parse_qsl
from Crypto.Cipher import AES
from botocore.auth import S3SigV4QueryAuth
from botocore.awsrequest import AWSRequest
from botocore.credentials import Credentials
p=argparse.ArgumentParser();p.add_argument('--original',type=Path,required=True);args=p.parse_args()
module=ast.parse(args.original.read_text(encoding='utf-8'))
cls=next(n for n in module.body if isinstance(n,ast.ClassDef) and n.name=='KickTheMapClient')
methods=[n for n in cls.body if isinstance(n,ast.FunctionDef) and n.name in ['_decrypt_aws_payload','_evp_bytes_to_key','_pkcs7_unpad']]
control=ast.Module(body=[ast.ClassDef(name='KickTheMapClient',bases=[],keywords=[],body=methods,decorator_list=[])],type_ignores=[]);ast.fix_missing_locations(control)
namespace=dict(json=json,base64=base64,hashlib=hashlib,AES=AES,KickTheMapError=RuntimeError)
exec(compile(control,'original-pure-protocol-control','exec'),namespace)
f=json.loads((Path(__file__).resolve().parents[1]/'rust/tests/fixtures/kickthemap/storage-control.json').read_text(encoding='utf-8'))
c=namespace['KickTheMapClient']._decrypt_aws_payload(f['payload']['adata'],f['payload']['thg'])
url=f"https://{c['bucket']}.s3.{c['region']}.amazonaws.com/"+quote(f['key'],safe='/')+'?'+urlencode({'response-content-disposition':f"attachment; filename=\"{f['filename']}\""},quote_via=quote)
request=AWSRequest(method='GET',url=url)
with patch('botocore.auth.get_current_datetime',return_value=datetime.datetime.strptime(f['date'],'%Y%m%dT%H%M%SZ')):
 S3SigV4QueryAuth(Credentials(c['aws_access_key'],c['aws_secret_access_key'],c['aws_session_token']),'s3',c['region'],expires=900).add_auth(request)
assert dict(parse_qsl(urlsplit(request.url).query))==dict(parse_qsl(urlsplit(f['url']).query))
print('Original Python decryption and independent botocore URL signature match the Rust fixture')
