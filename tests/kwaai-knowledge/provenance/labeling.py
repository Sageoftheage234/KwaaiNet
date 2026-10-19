from __future__ import annotations
import csv,random

from .metrics import false_rejection_rate,cohens_kappa,bootstrap_ci

def export_blinded(decisions,meta_db,out_csv,key_csv,n=150,seed=0):
    from .kwaainet_store import MetaReader
    chunk_texts={cid:c.get('text','') for cid,c in MetaReader(meta_db).chunks().items()}
    if not chunk_texts: raise ValueError(f'chunk table is empty in meta_db={meta_db}')
    r=random.Random(seed); groups={True:[],False:[]}
    for d in decisions: groups[d.accepted].append(d)
    each=min(n//2,len(groups[True]),len(groups[False])); sample=r.sample(groups[True],each)+r.sample(groups[False],each); r.shuffle(sample)
    with open(out_csv,'w',newline='') as f, open(key_csv,'w',newline='') as k:
        w=csv.writer(f); kw=csv.writer(k); w.writerow(['sample_id','fact_text','evidence_text','label']); kw.writerow(['sample_id','accepted','record_id'])
        for i,d in enumerate(sample): sid=f"S{i:04d}"; w.writerow([sid,d.text,' || '.join(chunk_texts.get(x,'') for x in d.evidence_chunk_ids),'']); kw.writerow([sid,d.accepted,d.record_id])

def import_labels(label_csv,key_csv,second_rater_csv=None,seed=0):
    with open(key_csv,newline='') as f: key={r['sample_id']:r for r in csv.DictReader(f)}
    def read(path):
        with open(path,newline='') as f: rows=list(csv.DictReader(f))
        out={}
        for r in rows:
            lab=r.get('label','').strip().lower()
            if lab not in {'supported','not_supported','unclear'}: raise ValueError(f"invalid label {lab!r} for {r.get('sample_id')}")
            out[r['sample_id']]=lab
        return out
    primary=read(label_csv); ids=[x for x in primary if x in key and primary[x]!='unclear']; labels=[primary[x] for x in ids]; accepted=[key[x]['accepted'].lower()=='true' for x in ids]
    stats=labeled_stats(labels,accepted,seed); stats['n_labeled']=len(ids); stats['n_unclear']=sum(v=='unclear' for v in primary.values())
    if second_rater_csv:
        second=read(second_rater_csv); common=[x for x in primary if x in second and primary[x]!='unclear' and second[x]!='unclear']; stats['cohens_kappa']=agreement([primary[x] for x in common],[second[x] for x in common]); stats['n_double_rated']=len(common)
    return stats

def agreement(labels1,labels2=None): return None if labels2 is None else cohens_kappa(labels1,labels2)
def labeled_stats(labels,accepted,seed=0):
    frr=false_rejection_rate(labels,accepted); supported=[(l,a) for l,a in zip(labels,accepted) if l=='supported']; ci=bootstrap_ci(supported,lambda xs: sum(not a for _,a in xs)/len(xs) if xs else 0,seed); rejected=[i for i,a in enumerate(accepted) if not a]; precision=sum(labels[i]=='not_supported' for i in rejected)/len(rejected) if rejected else 0.0; pci=bootstrap_ci(rejected,lambda xs: sum(labels[i]=='not_supported' for i in xs)/len(xs) if xs else 0,seed) if rejected else (0.0,0.0); return {'false_rejection_rate':frr,'false_rejection_ci95':ci,'gate_precision':precision,'gate_precision_ci95':pci}

