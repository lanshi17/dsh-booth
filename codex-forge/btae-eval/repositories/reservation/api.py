def book(service, request):
    booking = service.book(request["resource"], request["start"], request["end"])
    return {"resource": booking.resource, "start": booking.start, "end": booking.end}
