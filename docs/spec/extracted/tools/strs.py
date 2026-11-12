import sys
d=open(sys.argv[1],'rb').read()
parts=d.split(b'\x00'); off=0
for p in parts:
    o=off; off+=len(p)+1
    if len(p)<3: continue
    pr=sum(32<=c<127 for c in p)
    if pr<3 or pr/len(p)<0.6: continue
    if not any(65<=c<=122 for c in p): continue
    s=''
    for c in p:
        if 32<=c<127: s+=chr(c)
        elif c==0x1d: s+='{'
        elif c==0x1c: s+='}'
        elif c==0x16: s+='_'
        else: s+='<%02x>'%c
    print('%06x %s'%(o,s))
