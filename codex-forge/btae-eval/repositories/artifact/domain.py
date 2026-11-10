from dataclasses import dataclass

@dataclass(frozen=True)
class Artifact:
    name: str
    version: int
    content: str
