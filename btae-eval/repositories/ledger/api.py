def transfer(service, request):
    service.transfer(request["source"], request["target"], request["amount"])
    return {"ok": True}
