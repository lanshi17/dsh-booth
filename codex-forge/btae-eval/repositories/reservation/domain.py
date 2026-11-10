from dataclasses import dataclass

@dataclass(frozen=True)
class Booking:
    resource: str
    start: int
    end: int
