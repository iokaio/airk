---
id: management.service
title: Service and runtime
route: manage/service
area: management
aliases: start, stop, restart, upgrade, rollback, embedding, resources, Focus, Arbiter, Qwen, MiniLM
related_airk_help: service, upgrades, resources, models, arbiter
---
# Service and runtime

Stopping the AIRK Service and reviewing its runtime are in Management → Service. **Stop for restart** records a restart request and stops the service; the management page does not start a replacement. After it stops, run `airkctl service start` locally or use AIRK: Start AIRK Service in VS Code. Then run `airkctl admin open` for a new browser session or reopen management in the editor after verifying the service connection.

Opening **Service** reads current resources and Arbiter status automatically. **Refresh resources** and **Refresh Arbiter status** read them again. Resource summaries distinguish observed CPU and memory from configured reservations; unavailable measurements remain unavailable. Complete native results, including diagnostic fields, stay in **Details**. A retained Arbiter observation is labeled when a refresh fails.

Start at sign-in and restart/stop are separate task sections.

## Optional Arbiter stages

The approved source design separates **Focus** (Qwen3-Reranker-0.6B) from **Arbiter** (Qwen3-1.7B); the Large model (Qwen3-4B) was retired. Focus ranks MiniLM's retrieved candidates and can run with Arbiter off. Arbiter assesses evidence and guides bounded additional reads. These controls require a service version with stage support and qualified profiles; inspect the running service rather than assuming this guide enables them.

Both stages are off by default and need their own explicit index enrollment and verified supported assets. Model selection is intended configuration; active readiness additionally needs task/platform support and current resources. Status, ordinary queries and Help never authorize model downloads. Choose a model and use its **Download** action to request assets explicitly. Downloading does not enable a stage or enroll indexes. Select the indexes and tasks that stage may read, choose **Enable**, then **Apply** that stage's choices. Turning one stage off keeps the other stage's choices. Use **Cancel** or **Recover** for a failed model operation and refresh status before another change whose outcome is unknown.

Focus still has a memory and latency cost; candidate count and passage length matter. There is no universal RAM floor or assumed GPU support. An unavailable stage retains ordinary retrieval with an explicit reason, while normal vector/completeness refusals still apply. Disablement drains native work; remove models through guarded service controls instead of deleting live assets. Native Help's Arbiter guide explains requirements, offline use, stage consent, fallback and recovery.

The new 8192-token / 120-second default is an implementation target, not a claim that any optional profile passed its resource, latency or model-quality gates. Unqualified local-test profiles remain labeled as such; terminal decisions and final publication stay unavailable until their own evidence passes.
