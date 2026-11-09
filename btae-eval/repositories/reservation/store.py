class BookingStore:
    def __init__(self):
        self._bookings = []

    def add(self, booking):
        self._bookings.append(booking)

    def for_resource(self, resource):
        return [b for b in self._bookings if b.resource == resource]
