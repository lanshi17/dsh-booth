from dataclasses import dataclass

@dataclass(frozen=True)
class Job:
    key: str
    payload: str
