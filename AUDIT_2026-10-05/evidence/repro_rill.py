from pathlib import Path
import json, subprocess, zipfile, struct, tempfile
ROOT=Path(__file__).resolve().parents[1]/'outputs/rill-ml'
OUT=Path(__file__).resolve().parent/'rill-repro-evidence'
OUT.mkdir(exist_ok=True)
binary=ROOT/'target/debug/rill-runtime.exe'
def call(state, request):
    p=subprocess.run([str(binary),'preview-serve','--state',str(state),'--feature-schema-hash','a'*64,'--model-generation','2'],input=json.dumps(request)+'\n',text=True,encoding='utf-8',capture_output=True,timeout=15)
    if p.returncode: raise RuntimeError(p.stderr)
    return json.loads(p.stdout)
def req(kind,gen,ident,**kw):
    return dict(requestId=ident,apiVersion=3,clientIdentity={'name':'audit','version':'1'},partitionKey='p',capability='org.rill.preview.'+kind,featureSchemaHash='a'*64,modelGeneration=2,stateGeneration=gen,payloadLimit=1048576,request={'method':kind,**kw})
with tempfile.TemporaryDirectory(dir=OUT) as temp:
    state=Path(temp)/'time.json'
    d=call(state,req('decide',0,'d',context={'actions':[{'id':'a','features':[1.0]}]}))
    print('decision',d)
    f=call(state,req('feedback',d['stateGeneration'],'f',decisionId='d',selectedActionId='a',reward=1.0,outcomeTimeMs=0,generation=2))
    print('REPRO RML-02: outcome before decision was accepted:',f)
    state2=Path(temp)/'overflow.json'
    d=call(state2,req('decide',0,'big1',context={'actions':[{'id':'a','features':[1e308,1e308]}]}))
    f=call(state2,req('feedback',d['stateGeneration'],'big-f',decisionId='big1',selectedActionId='a',reward=1.0,outcomeTimeMs=999999999999999,generation=2))
    d2=call(state2,req('decide',f['stateGeneration'],'big2',context={'actions':[{'id':'a','features':[1e308,1e308]}]}))
    print('REPRO RML-03: finite input yielded nonnumeric score:',d2)

pack=OUT/'underdeclared.rillpack'
with zipfile.ZipFile(pack,'w',zipfile.ZIP_DEFLATED) as z:
    z.writestr('manifest.json',' '*(1024*1024)+'{}')
b=bytearray(pack.read_bytes())
struct.pack_into('<I',b,22,1)
central=b.index(b'PK\x01\x02')
struct.pack_into('<I',b,central+24,1)
pack.write_bytes(b)
p=subprocess.run([str(ROOT/'target/debug/rill-pack.exe'),'verify','--pack',str(pack),'--key-id','audit','--public-key-hex','01'*32],text=True,encoding='utf-8',capture_output=True)
print('REPRO RML-01: 1MiB actual manifest (declared=1 byte) reached JSON parsing:',p.returncode,p.stderr.strip())
