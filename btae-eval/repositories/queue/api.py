def submit(service, request):
    job = service.submit(request["key"], request["payload"])
    return {"key": job.key, "payload": job.payload}
