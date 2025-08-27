# FastAPI gateway for R&D GPT agents
import os
import uuid
import asyncio
from datetime import datetime
from typing import Optional, Dict, Any, Literal

from fastapi import FastAPI, Header, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel, Field
import httpx

API_KEY = os.getenv("AGENTS_API_KEY", "dev-key-change-me")

app = FastAPI(title="R&D Agents Gateway", version="1.0.0")

# Allow CORS for quick testing (tighten in prod)
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=False,
    allow_methods=["*"],
    allow_headers=["*"],
)

class TaskPayload(BaseModel):
    agent_id: Literal[
        "coordinateur","chercheur","experimentateur",
        "analyste","architecte","securite","documentariste"
    ]
    task: str = Field(..., description="But/opération demandée")
    context: Dict[str, Any] = Field(default_factory=dict)

class Ack(BaseModel):
    status: Literal["accepted","queued","error"]
    task_id: str
    agent_id: str
    received_at: datetime
    details: Optional[Dict[str, Any]] = None

class ResultPayload(BaseModel):
    task_id: str
    agent_id: Literal[
        "coordinateur","chercheur","experimentateur",
        "analyste","architecte","securite","documentariste"
    ]
    status: Literal["success","failed"]
    metrics: Optional[Dict[str, float]] = None
    artifacts: Optional[Dict[str, Any]] = None
    notes: Optional[str] = None

AGENT_ROUTES = {
    "coordinateur": os.getenv("N8N_COORD_URL", "https://REPLACE-ME/coordinateur"),
    "chercheur": os.getenv("N8N_CHERCH_URL", "https://REPLACE-ME/chercheur"),
    "experimentateur": os.getenv("N8N_EXPE_URL", "https://REPLACE-ME/experimentateur"),
    "analyste": os.getenv("N8N_ANALY_URL", "https://REPLACE-ME/analyste"),
    "architecte": os.getenv("N8N_ARCHI_URL", "https://REPLACE-ME/architecte"),
    "securite": os.getenv("N8N_SECU_URL", "https://REPLACE-ME/securite"),
    "documentariste": os.getenv("N8N_DOCU_URL", "https://REPLACE-ME/documentariste"),
}

def require_key(x_api_key: Optional[str]):
    if x_api_key != API_KEY:
        raise HTTPException(401, "Invalid API key")

@app.get("/")
async def root():
    return {"ok": True, "service": "rd-agents-gateway", "time": datetime.utcnow().isoformat()+"Z"}

@app.post("/tasks", response_model=Ack)
async def submit_task(payload: TaskPayload, x_api_key: Optional[str] = Header(None)):
    require_key(x_api_key)
    if payload.agent_id not in AGENT_ROUTES:
        raise HTTPException(400, "Unknown agent_id")

    task_id = str(uuid.uuid4())

    async def forward():
        url = AGENT_ROUTES[payload.agent_id]
        async with httpx.AsyncClient(timeout=30) as client:
            try:
                await client.post(
                    url,
                    json={"task_id": task_id, **payload.model_dump()},
                    headers={"X-API-Key": API_KEY}
                )
            except Exception as e:
                # Minimal logging
                print(f"[forward error] agent={payload.agent_id} url={url} err={e}")

    asyncio.create_task(forward())

    return Ack(
        status="accepted",
        task_id=task_id,
        agent_id=payload.agent_id,
        received_at=datetime.utcnow(),
        details={"forward_to": AGENT_ROUTES[payload.agent_id]},
    )

@app.post("/results", status_code=204)
async def push_result(result: ResultPayload, x_api_key: Optional[str] = Header(None)):
    require_key(x_api_key)
    # Hook: persist to DB here if needed
    print("[result]", result.model_dump())
    return
