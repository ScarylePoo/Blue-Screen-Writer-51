import sys,struct
d=open('/mnt/i/Claude Space/wp51/WPHELP.FIL','rb').read()
nsec=struct.unpack_from('<H',d,0x10)[0]
secs=[struct.unpack_from('<I',d,0x12+4*i)[0] for i in range(nsec)]
def nibstream(start):
    i=start*2
    while True:
        b=d[i//2]; yield (b>>4) if i%2==0 else (b&15); i+=1
def decode(sec_i, idx):
    s=secs[sec_i]; n=struct.unpack_from('<H',d,s)[0]; tab=d[s+2:s+17].decode('latin1')
    offs=[struct.unpack_from('<H',d,s+17+2*k)[0] for k in range(n)]
    base=s+2
    g=nibstream(base+offs[idx]+1); out=[0x10,5,0,0x1d,d[base+offs[idx]]]
    while True:
        n_=next(g)
        if n_<15: out.append(ord(tab[n_]))
        else:
            c=(next(g)<<4)|next(g); out.append(c)
            if c==0: break
        if len(out)>6000: break
    return bytes(out)
def render(bs):
    grid=[[' ']*80 for _ in range(25)]; att=[[0]*80 for _ in range(25)]
    r=c=0; a=0; i=0; extra=[]
    while i<len(bs):
        x=bs[i]
        if x==0x10:
            cc=bs[i+1]; rr=bs[i+2]; i+=3
            c = c if cc in (0xfe,0xff) else cc
            r = r if rr in (0xfe,0xff) else rr
            continue
        if x==0x1d: a|=1
        elif x==0x1c: a&=~1
        elif x==0x14: a|=2
        elif x==0x15: a&=~2
        elif x==0x16: a|=4  # mnemonic marker?
        elif x==0: break
        elif x==0x0a: r+=1; c=0
        elif x==0x0d: c=0
        elif x<0x20: extra.append('<%02x>@%d,%d'%(x,r,c))
        else:
            ch=bytes([x]).decode('cp437')
            if 0<=r<25 and 0<=c<80: grid[r][c]=ch; att[r][c]=a
            if a&4: a&=~4
            c+=1
        i+=1
    lines=[]
    for r in range(25):
        s='';cur=0
        for cc in range(80):
            A=att[r][cc] if grid[r][cc]!=' ' or True else 0
            if A!=cur:
                if cur&1 and not A&1: s+='}'
                if cur&2 and not A&2: s+='»'
                if A&1 and not cur&1: s+='{'
                if A&2 and not cur&2: s+='«'
                if A&4: s+='_'
                cur=A&3
            elif A&4: s+='_'
            s+=grid[r][cc]
        if cur&1: s+='}'
        if cur&2: s+='»'
        lines.append(s.rstrip())
    while lines and not lines[-1]: lines.pop()
    return '\n'.join(lines), extra
for si,s in enumerate(secs):
    n=struct.unpack_from('<H',d,s)[0]
    for k in range(n):
        try:
            t,e=render(decode(si,k))
        except Exception as ex:
            t,e='ERR %s'%ex,[]
        print('='*20,'SECTION',si,'SCREEN',k, ' '.join(e[:10]))
        print(t)
