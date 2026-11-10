def publish(service, request):
    artifact = service.publish(request["name"], request["version"], request["content"])
    return {"name": artifact.name, "version": artifact.version}
