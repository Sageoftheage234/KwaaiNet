from __future__ import annotations
import re
from typing import Protocol, Sequence

class SupportScorer(Protocol):
    name: str
    def score(self, premise: str, hypothesis: str) -> float: ...
    def score_batch(self, pairs: Sequence[tuple[str, str]]) -> list[float]: ...

class LexicalScorer:
    """Deterministic token-overlap scorer for tests and smoke runs only."""
    name = "lexical_NOT_FOR_REPORTING"
    def score(self, premise: str, hypothesis: str) -> float:
        tok=lambda s:set(re.findall(r"[a-z0-9]+",s.lower()))
        h=tok(hypothesis)
        return len(tok(premise)&h)/len(h) if h else 0.0
    def score_batch(self, pairs):
        return [self.score(p,h) for p,h in pairs]

class NLIScorer:
    """Lazy HuggingFace NLI scorer using maximum entailment over premise windows."""
    def __init__(self, model_name="MoritzLaurer/DeBERTa-v3-large-mnli-fever-anli-ling-wanli", window=384, stride=256, batch_size=8):
        if window <= 0 or stride <= 0 or batch_size <= 0:
            raise ValueError("window, stride, and batch_size must be positive")
        self.model_name=model_name; self.window=window; self.stride=stride; self.batch_size=batch_size
        self.name=f"nli:{model_name}"; self._pipe=None
    def _load(self):
        if self._pipe is None:
            from transformers import pipeline
            self._pipe=pipeline("text-classification",model=self.model_name,return_all_scores=True)
    def _windows(self,premise):
        words=premise.split()
        if not words: return [""]
        return [" ".join(words[i:i+self.window]) for i in range(0,len(words),self.stride)]
    @staticmethod
    def _entailment(out):
        return max((float(x["score"]) for x in out if "entail" in str(x["label"]).lower() or str(x["label"]).upper()=="LABEL_2"),default=0.0)
    def score_batch(self,pairs):
        self._load(); flat=[]; owners=[]
        for idx,(premise,hypothesis) in enumerate(pairs):
            for p in self._windows(premise): flat.append({"text":p,"text_pair":hypothesis}); owners.append(idx)
        if not flat: return []
        outputs=self._pipe(flat,batch_size=self.batch_size)
        best=[0.0]*len(pairs)
        for owner,out in zip(owners,outputs): best[owner]=max(best[owner],self._entailment(out))
        return best
    def score(self,premise,hypothesis):
        return self.score_batch([(premise,hypothesis)])[0]
