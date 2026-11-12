import struct
d=open('/mnt/i/Claude Space/wp51/WP.FIL','rb').read()
import sys; B=int(sys.argv[1],0)
tab=d[B+2:B+17]
n=struct.unpack_from("<H",d,B)[0]; offs=[struct.unpack_from('<H',d,B+17+2*k)[0] for k in range(n)]
def dec(pos,limit=6000):
    out=bytearray();i=pos*2
    def nib(i): b=d[i//2]; return (b>>4) if i%2==0 else b&15
    while len(out)<limit and i//2+1<len(d):
        x=nib(i)
        if x<15:
            if tab[x]==0: break
            out.append(tab[x]); i+=1
        else:
            c=(nib(i+1)<<4)|nib(i+2); i+=3
            if c==0: break
            out.append(c)
    return bytes(out)
def show(b):
    s=''
    for c in b:
        if 32<=c<127: s+=chr(c)
        elif c==0x1d: s+='{'
        elif c==0x1c: s+='}'
        elif c==0x16: s+='_'
        elif c>=0x80: s+=bytes([c]).decode('cp437')
        else: s+='<%02x>'%c
    return s
def render(bs):
    grid=[[' ']*80 for _ in range(26)]
    r=c=0;i=0;bold=False;mn=False
    while i<len(bs):
        x=bs[i]
        if x==0x10 and i+2<len(bs):
            cc=bs[i+1]; rr=bs[i+2]; i+=3
            if cc!=0xfe and cc!=0xff: c=cc
            if rr==0xfe: pass
            elif rr!=0xff: r=rr
            continue
        i+=1
        if x==0x0a: r+=1; c=0; continue
        if x==0x0b: continue
        if x==0x1d: bold=True; continue
        if x==0x1c: bold=False; continue
        if x==0x16: mn=True; continue
        if x<0x20: continue
        ch=bytes([x]).decode('cp437')
        if 0<=r<26 and 0<=c<80:
            grid[r][c]= ch
        c+=1
    lines=[''.join(g).rstrip() for g in grid]
    while lines and not lines[-1]: lines.pop()
    return '\n'.join(lines)
for k,o in enumerate(offs):
    if o==0xffff: continue
    b=dec(B+o)
    print('#### STR %d'%k)
    print('RAW:',show(b))
    if b.count(0x10)>=3:
        print('RENDER:'); print(render(b))
