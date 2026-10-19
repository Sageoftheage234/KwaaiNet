from __future__ import annotations
import random
from collections import Counter
from .schemas import FactSupport

def _rate(items, pred):
    """Return the share of items satisfying pred."""
    return sum(bool(pred(x)) for x in items) / len(items) if items else 0.0

def traceability(supports: list[FactSupport]) -> float:
    """Share of facts classified traceable."""
    return _rate(supports, lambda x: x.support_class == "traceable")

def unsupported_rate(supports: list[FactSupport]) -> float:
    """Share of facts classified unsupported."""
    return _rate(supports, lambda x: x.support_class == "unsupported")

def misattribution_rate(supports: list[FactSupport]) -> float:
    """Share of facts classified misattributed."""
    return _rate(supports, lambda x: x.support_class == "misattributed")

def false_rejection_rate(labels: list[str], accepted: list[bool]) -> float:
    """Supported rejected divided by all human-labeled supported facts."""
    supported=[i for i,x in enumerate(labels) if x == "supported"]
    return sum(not accepted[i] for i in supported)/len(supported) if supported else 0.0

def bootstrap_ci(values, statistic, seed=0, n_boot=2000, alpha=.05):
    """Seeded percentile bootstrap confidence interval."""
    if not values: return (0.0,0.0)
    r=random.Random(seed); stats=[]; n=len(values)
    for _ in range(n_boot): stats.append(statistic([values[r.randrange(n)] for _ in range(n)]))
    stats.sort(); return stats[int(alpha/2*n_boot)], stats[min(n_boot-1,int((1-alpha/2)*n_boot))]

def cohens_kappa(a: list[str], b: list[str]) -> float:
    """Unweighted Cohen kappa for paired categorical labels."""
    if len(a)!=len(b): raise ValueError("rater lengths differ")
    if not a: return 0.0
    po=sum(x==y for x,y in zip(a,b))/len(a); ca=Counter(a); cb=Counter(b)
    pe=sum(ca[k]/len(a)*cb[k]/len(b) for k in set(ca)|set(cb))
    return 1.0 if pe==1 and po==1 else (po-pe)/(1-pe) if pe<1 else 0.0
