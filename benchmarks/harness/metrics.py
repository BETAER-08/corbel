from collections import Counter
from dataclasses import dataclass
from typing import List, Optional, Tuple


@dataclass
class PRF:
    tp: int
    fp: int
    fn: int
    precision: Optional[float]
    recall: Optional[float]
    f1: Optional[float]


def score(predicted: List, ground_truth: List) -> PRF:
    pred_counts = Counter(predicted)
    truth_counts = Counter(ground_truth)
    overlap = pred_counts & truth_counts
    tp = sum(overlap.values())
    fp = sum(pred_counts.values()) - tp
    fn = sum(truth_counts.values()) - tp

    precision = tp / (tp + fp) if (tp + fp) > 0 else None
    recall = tp / (tp + fn) if (tp + fn) > 0 else None
    if precision is not None and recall is not None and (precision + recall) > 0:
        f1 = 2 * precision * recall / (precision + recall)
    elif precision == 0 or recall == 0:
        f1 = 0.0
    else:
        f1 = None

    return PRF(tp=tp, fp=fp, fn=fn, precision=precision, recall=recall, f1=f1)


def caller_key(entry) -> Tuple[str, str]:
    return (entry["enclosing_symbol"], entry["file"])


def callee_key(name) -> str:
    return name


def definition_key(entry) -> Tuple[str, int]:
    return (entry["file"], entry["line"])


def diff(predicted: List, ground_truth: List):
    pred_counts = Counter(predicted)
    truth_counts = Counter(ground_truth)
    overlap = pred_counts & truth_counts
    matched = list(overlap.elements())
    missing = list((truth_counts - pred_counts).elements())
    extra = list((pred_counts - truth_counts).elements())
    return matched, missing, extra


def aggregate(prf_list: List[PRF]) -> PRF:
    tp = sum(p.tp for p in prf_list)
    fp = sum(p.fp for p in prf_list)
    fn = sum(p.fn for p in prf_list)
    precision = tp / (tp + fp) if (tp + fp) > 0 else None
    recall = tp / (tp + fn) if (tp + fn) > 0 else None
    if precision is not None and recall is not None and (precision + recall) > 0:
        f1 = 2 * precision * recall / (precision + recall)
    elif precision == 0 or recall == 0:
        f1 = 0.0
    else:
        f1 = None
    return PRF(tp=tp, fp=fp, fn=fn, precision=precision, recall=recall, f1=f1)
