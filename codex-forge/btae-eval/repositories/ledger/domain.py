from dataclasses import dataclass

@dataclass
class Account:
    key: str
    balance: int = 0
