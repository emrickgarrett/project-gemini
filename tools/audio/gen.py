# Regenerates the arena one-shots and loops (seed 7).
# Run from this directory, then copy the .ogg files to assets/audio/sfx/.
import numpy as np, wave, subprocess
SR=48000; rng=np.random.default_rng(7)
def lp(x,a):
    y=np.empty_like(x); s=0.0
    for i,v in enumerate(x): s+=a*(v-s); y[i]=s
    return y
def save(name,x,peak=0.7):
    x=x/ (np.max(np.abs(x))+1e-9)*peak
    d=(x*32767).astype(np.int16)
    with wave.open(name+'.wav','wb') as w:
        w.setnchannels(1);w.setsampwidth(2);w.setframerate(SR);w.writeframes(d.tobytes())
    subprocess.run(['ffmpeg','-y','-loglevel','error','-i',name+'.wav','-c:a','libvorbis','-q:a','5',name+'.ogg'])
def env(n,att,dec):
    t=np.arange(n)/SR; return np.minimum(t/att,1)*np.exp(-t/dec)
def step(kind,v):
    n=int(SR*0.25); t=np.arange(n)/SR; nz=rng.standard_normal(n)
    if kind=='dirt': x=lp(nz,0.08+0.02*v)*env(n,.003,.04)+0.5*np.sin(2*np.pi*(70+5*v)*t)*env(n,.002,.03)
    if kind=='rock': x=lp(nz,0.25+0.03*v)*env(n,.001,.025)+0.4*np.sin(2*np.pi*(140+10*v)*t)*env(n,.001,.02)
    if kind=='metal':
        f=[310+15*v,733+20*v,1290+30*v]; x=sum(np.sin(2*np.pi*q*t)/(i+1) for i,q in enumerate(f))*env(n,.001,.09)+0.6*lp(nz,.4)*env(n,.001,.01)
    if kind=='resin': x=lp(nz,0.05)*env(n,.01,.06)*(1+0.5*np.sin(2*np.pi*(30+3*v)*t))+0.3*np.sin(2*np.pi*55*t)*env(n,.005,.05)
    return x
for k in ['dirt','metal','rock','resin']:
    for v in range(4): save(f'step_{k}_{v+1}',step(k,v),0.6)
def loop(x,fade):  # crossfade end into start for seamless loop
    f=int(SR*fade); a=np.linspace(0,1,f); y=x[:-f].copy(); y[:f]=y[:f]*a+x[-f:]*(1-a); return y
# arena bed: 12s wind + low drone
n=SR*13; t=np.arange(n)/SR
wind=lp(rng.standard_normal(n),0.01)*(1+0.4*np.sin(2*np.pi*t/6.5))
bed=wind+0.25*np.sin(2*np.pi*48*t)+0.1*np.sin(2*np.pi*72.3*t)
save('arena_01_bed',loop(bed,1.0),0.5)
# hive nest idle: steady low, no rising pitch, no hiss/clicks
n=SR*9; t=np.arange(n)/SR
nest=np.sin(2*np.pi*41*t)*(0.8+0.2*np.sin(2*np.pi*t/4.0))+0.3*np.sin(2*np.pi*61.5*t)+0.15*lp(rng.standard_normal(n),0.004)
save('hive_nest_idle',loop(nest,1.0),0.5)
# relay tower hum
n=SR*7; t=np.arange(n)/SR
hum=sum(np.sin(2*np.pi*60*h*t)/h for h in range(1,6))*(1+0.05*np.sin(2*np.pi*t/3.5))
save('relay_tower_hum',loop(hum,0.5),0.45)
# beacon pulse one-shot
n=int(SR*0.9); t=np.arange(n)/SR
b=(np.sin(2*np.pi*880*t)+0.4*np.sin(2*np.pi*1320*t))*env(n,.01,.25)
save('beacon_pulse',b,0.5)
