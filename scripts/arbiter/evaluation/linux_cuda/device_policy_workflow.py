# SPDX-License-Identifier: Apache-2.0
"""Exercise the original managed service and device selectors using the normal matched CLI."""
import json


def exercise(root, fixture, record, check, *, existing_index=None):
    record("managed-start",json.loads((root/"managed-start.json").read_bytes()))
    index = existing_index
    if index is None:
        command = ["--output","json","--service-runtime-dir",fixture.control,"--root",root/"source",
                   "--data-dir",root/"data","library","setup","--name","Device check","--watch","off","--apply"]
        setup = fixture.run(*command,check=False)
        parsed = [json.loads(line) for line in setup.stdout.splitlines() if line.strip()]
        record("catalog-setup",{"exit":setup.returncode,"responses":parsed,"stderr":setup.stderr})
        check(setup.returncode == 0 and parsed[-1]["status"] == "catalog_index_setup_applied"
              and parsed[-1]["receipt"]["stage"] == "ready","native reviewed catalog setup failed")
        index = parsed[-1]["receipt"]["index_id"]
    else:
        listed = fixture.run("--output","json","--service-runtime-dir",fixture.control,
                             "library","list","--kind","indexes","--all")
        pages = [json.loads(line) for line in listed.stdout.splitlines() if line.strip()]
        record("existing-catalog-pages",pages)
        check(all(p["status"]=="catalog_page" for p in pages) and
              any(i["kind"]=="index" and i["value"]["id"]==index for p in pages for i in p["page"]["items"]),
              "original ready index missing from native catalog")
    def arbiter(label,request,*,refusal=None):
        op = request["operation"]
        if op == "status_device":
            arguments = ["status"]
        elif op == "initialize_device":
            arguments = ["initialize"]
        else:
            req = root/(label+"-request.json")
            with req.open("x") as file:
                json.dump(request,file,separators=(",",":"))
            arguments = ["configure","--file",req]
        actual = fixture.run("--output","json","--service-runtime-dir",fixture.control,"arbiter",*arguments,
                             check=False)
        observed = record(label,{"exit":actual.returncode,"stdout":actual.stdout,"stderr":actual.stderr})
        if refusal is not None:
            check(observed["exit"] != 0 and json.loads(observed["stderr"])["code"] == refusal,
                  "unexpected native result for unqualified stage activation")
            return
        check(observed["exit"] == 0,"matched arbiter CLI refused "+op+": "+actual.stderr)
        return json.loads(observed["stdout"])
    def resources(label,request):
        response = record(label,fixture.rpc(request))
        check(response["status"] == "resources" and response["configuration_error"] is False,
              "native resource read or write reported an error")
        return response["observation"]["configuration"]["allow_gpu"]
    status = arbiter("initial",{"operation":"status_device"})
    if status.get("initialization") is not None:
        status = arbiter("initialize",{"operation":"initialize_device","expected_legacy_configuration":None})
    def configure(previous,policy,label):
        settings = {"enabled":False,"selected_model":"arbiter","device_policy":policy,"gpu_precision":"fp16",
                    "indexes":[{"index_id":index,"tasks":["airk.context_action.v1"]}]}
        response = arbiter(label,{"operation":"configure_device","stage":"context",
                           "expected_revision":previous["configuration"]["revision"],"configuration":settings})
        check(response["configuration"]["context"] == settings,"native stage selection or enrollment changed")
        return response
    auto = configure(status,"auto","auto-fp16")
    check(resources("initial-resources","resources") is True,"fresh global GPU default differs")
    for allowed in (False,True):
        label = "enabled" if allowed else "disabled"
        setting = {"set_gpu_allowed":{"allow_gpu":allowed}}
        check(resources("set-"+label,setting) is allowed,"native setting write failed")
        check(resources("get-"+label,"resources") is allowed,"native setting read differs")
        current = arbiter("arbiter-"+label,{"operation":"status_device"})
        check(current["configuration"] == auto["configuration"],"global GPU setting changed stage selections")
    configured = auto["configuration"]
    arbiter("enable-unqualified",{"operation":"configure_device","stage":"context",
            "expected_revision":configured["revision"],"configuration":dict(configured["context"],enabled=True)},
            refusal="arbiter_unavailable")
    unchanged = arbiter("after-refusal",{"operation":"status_device"})
    check(unchanged["configuration"] == configured,"refusal changed native stage configuration")
    cpu = configure(unchanged,"cpu","same-model-cpu")
    check(cpu["context"]["selected_model"] == auto["context"]["selected_model"]
          and cpu["context"]["device_policy"] == "cpu","same-model explicit CPU choice changed")
    check(all(s["context"]["active_profile"] is None for s in (cpu,auto)),
          "unexpected active profile in a descriptive zero-row product")
