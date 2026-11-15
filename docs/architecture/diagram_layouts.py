# SPDX-License-Identifier: Apache-2.0
"""Explicit print layouts. Blue: components; teal: authority; amber: optional."""
TEAL='#087f82';BLUE='#2563a6';AMBER='#a66312';MUTED='#50647b'

def figures(D):
    d=D(1,'One product, several ways to use it','People manage and inspect. AI agents request scoped evidence from the same service.')
    d.box(60,195,660,120,'People',['Choose scope; inspect evidence; manage indexes.'],TEAL,'#edf9f7')
    d.box(1100,195,440,120,'External AI agents',['Ask for bounded evidence.'],TEAL,'#edf9f7')
    d.box(60,450,660,180,'Shared browser + editor UI',['Management and Search','Guided evidence, Review, Strategies / Planner','Editor-only workspace actions stay guarded.'])
    d.box(780,450,330,180,'airkctl',['Commands / diagnostics','Session / tool shell','Setup and recovery'])
    d.box(1170,450,370,180,'HTTPS / MCP',['Authenticated evidence','Connection scope','Complete native results'])
    d.box(160,805,1280,140,'AIRK service',['Owns reads, index jobs, publication, management and local inference.','Original citations, gaps, continuations and errors remain inspectable.'],TEAL,'#edf9f7')
    d.arrow([(390,315),(390,450)]);d.arrow([(720,255),(945,255),(945,450)]);d.arrow([(1320,315),(1320,450)])
    for x in [390,945,1355]:d.arrow([(x,630),(x,805)],both=True)
    yield '01-product-interaction',d

    d=D(2,'Two executables; one resident service','The shared UI is a client. Owners inside the service run in that one process.')
    d.box(60,195,425,140,'VS Code window',['Extension host + shared UI','One window-owned helper'])
    d.box(595,195,425,140,'airkctl session',['I2 framed stdio protocol 2','Exits with its editor owner'])
    d.box(1130,195,410,140,'Browser / AI agents',['Browser: I3 Admin','Agents: HTTPS evidence'])
    d.arrow([(485,265),(595,265)],'I2',(540,245),both=True)
    d.box(595,415,425,110,'airkctl one-shots',['Commands exit when finished.'])
    d.group(60,650,1480,310,'ONE RESIDENT airk PROCESS PER USER + INSTALLATION','#f4f8fc',BLUE)
    d.box(90,725,320,155,'HTTPS / Admin',['I3 / native Admin','MCP evidence ingress'])
    d.box(455,725,320,155,'Service / catalog',['I1 local control','Grants, jobs, lifecycle'])
    d.box(820,725,320,155,'Native workers',['Index / query / cleanup','In-process runtimes'])
    d.box(1185,725,320,155,'Local inference',['Shared MiniLM','Optional Arbiter'],AMBER,'#fff7e9')
    d.arrow([(807,335),(807,375),(430,375),(430,600),(615,600),(615,725)])
    d.arrow([(807,525),(807,600),(700,600),(700,725)])
    d.arrow([(1335,335),(1335,570),(250,570),(250,725)],'HTTPS',(1300,550))
    d.text(95,930,'Matched pair: airk + airkctl. No resident process per AI agent or model.',22,MUTED)
    yield '02-processes-and-planes',d

    d=D(3,'Native components and dependency direction','Selected production dependencies; all 19 crate owners and direct edges are explained in the guide.')
    d.box(60,190,960,115,'airk executable to airk-service',['Thin entry and resident composition; private setup / login roles.'])
    d.box(1130,190,410,115,'airkctl',['User CLI + I2 session'])
    for x,title,line in [(60,'airk-https','HTTP / Admin adapters'),(450,'airk-frontend','Scope / combined reads'),(840,'airk-supervisor','Worker lifetime'),(1230,'Service owners','Catalog / inference / setup')]:
        d.box(x,445,310,125,title,[line]);d.arrow([(540,305),(540,375),(x+155,375),(x+155,445)])
    d.box(450,685,700,115,'airk-mcp to airk-kernel',['Canonical handlers to publication, gathering and queries'])
    d.arrow([(605,570),(605,685)]);d.arrow([(995,570),(995,685)])
    d.box(1130,820,410,110,'airk-ipc',['Same-user I1 frames'])
    d.arrow([(1490,305),(1570,305),(1570,785),(1335,785),(1335,820)])
    d.box(60,820,960,110,'airk-ingest / airk-store / airk-retrieval',['Extraction; owned lexical / DiskANN state; retrieval / inference'])
    d.arrow([(800,800),(800,820)])
    d.box(60,945,1480,50,'No AIRK dependencies: airk-core contracts + airk-process platform primitives',[],TEAL,'#edf9f7')
    yield '03-native-ownership',d

    d=D(4,'Read-only sources; owned derived state','The workspace is authoritative. Publication and cleanup mutate only AIRK-owned state.')
    d.box(60,200,450,160,'Selected sources',['Original code / documents','Ignore and privacy admission','No source writes'],TEAL,'#edf9f7')
    d.box(575,200,450,160,'Ingest',['Discover, hash, parse, extract','Facts and original locations','Bounded document conversion'])
    d.box(1090,200,450,160,'Derived staging',['Artifacts / chunks / embeddings','Content and object identities','Quota and recovery checks'])
    d.arrow([(510,280),(575,280)]);d.arrow([(1025,280),(1090,280)])
    d.box(1090,570,450,160,'Kernel publication',['Prepared / Applying journal','Current revision and binding','Owned publication transition'])
    d.box(575,570,450,160,'Per-index Store',['SQLite / FTS5 and facts','DiskANN-backed vector state','Manifests and object leases'])
    d.box(60,570,450,160,'Leased query reads',['Current publication snapshot','Lexical / vector retrieval','Root and revision isolation'])
    d.arrow([(1315,360),(1315,570)]);d.arrow([(1090,650),(1025,650)]);d.arrow([(575,650),(510,650)])
    d.box(60,865,800,95,'Saved-source verification',['Bounded live bytes compared with cited indexed bytes.'],TEAL,'#edf9f7')
    d.box(940,865,600,95,'Owned maintenance',['Watching, indexing and guarded cleanup.'])
    d.arrow([(60,280),(30,280),(30,910),(60,910)],dashed=True,color=TEAL)
    d.arrow([(800,730),(800,800),(1240,800),(1240,865)])
    yield '04-source-and-publication',d

    d=D(5,'One admitted read; bounded Answer; complete delivery','Optional local advice chooses verified gathering actions. It never executes an agent or command.')
    d.box(60,195,450,160,'Original admission',['Authenticate and resolve scope','Earliest deadline / cancellation','Typed arguments and lifetime'])
    d.box(575,195,450,160,'Canonical MCP + frontend',['Shared read handlers','Ordinary reads / Workbench','Answer: Context /2 contract'])
    d.box(1090,195,450,160,'Partition first',['Exact immutable-index roster','Shared calls / tokens / bytes','Global assessment ordinals'])
    d.arrow([(510,275),(575,275)]);d.arrow([(1025,275),(1090,275)])
    d.box(60,520,760,170,'Each local Kernel gathers evidence once',['Canonical records and exact citations','Deterministic fallback when advice is absent','Local coverage never suppresses selected siblings.'])
    d.box(930,520,610,170,'Optional per-index Arbiter',['Default off; explicit verified enrollment','Native cards; at most 3 assessments','Availability and qualification stay distinct.'],AMBER,'#fff7e9')
    d.arrow([(1315,355),(1315,435),(440,435),(440,520)])
    d.arrow([(820,605),(930,605)],both=True,dashed=True,color=AMBER)
    d.box(60,820,450,140,'Whole composition',['All members and full results','Unknown work holds capacity','One original global budget'])
    d.box(575,820,450,140,'Whole-source proof',['Report / Require / Off','Shared bounded native receipt','No source-claim fabrication'],TEAL,'#edf9f7')
    d.box(1090,820,450,140,'Actual delivery cut',['Live grant / source / policy','Provenance, gaps, continuations','No uncertain execution replay'],TEAL,'#edf9f7')
    d.arrow([(440,690),(440,820)]);d.arrow([(510,890),(575,890)]);d.arrow([(1025,890),(1090,890)])
    yield '05-query-and-answer',d

    d=D(6,'Separate scope owners; native isolation','Preferences are not authority. Workspace defaults are not access-control lists.')
    d.box(60,205,460,240,'AI connection scope',['Owner-reviewed defaults','Choice off: assigned / narrower','Choice on: any index owned by','the same OS user.'],TEAL,'#edf9f7')
    d.box(570,205,460,240,'Editor window scope',['W4 grant + live window lease','Checked index / set selection','Close or expiry revokes authority.','Agent defaults are separate.'])
    d.box(1080,205,460,240,'Admin session scope',['A2 grant + actual session','Browser cookie / anti-forgery','Editor: private native bridge','Credentials stay out of exports.'])
    d.box(160,620,1280,140,'Native authority and delivery owners',['Bind installation, service instance, namespace, immutable indexes and publication.','Recheck scope, source policy, cancellation and optional-model policy at delivery.'],TEAL,'#edf9f7')
    for x in [290,800,1310]:d.arrow([(x,445),(x,620)])
    d.box(60,880,460,80,'Index A',['Own roots, revision and state'])
    d.box(570,880,460,80,'Index B',['Own roots, revision and state'])
    d.box(1080,880,460,80,'Index C',['Requires actual authority'])
    for x in [290,800,1310]:d.arrow([(x,760),(x,880)])
    yield '06-scope-and-trust',d
