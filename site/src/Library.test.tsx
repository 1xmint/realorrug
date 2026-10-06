// SPDX-License-Identifier: Apache-2.0
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Router } from "wouter";
import { memoryLocation } from "wouter/memory-location";
import { Library, TokenDossier } from "./Library";
const address="0x1111111111111111111111111111111111111111";
const response=(value:unknown,status=200)=>Promise.resolve(new Response(JSON.stringify(value),{status}));
afterEach(()=>{cleanup();vi.unstubAllGlobals();});
it("keeps public browsing available and signed-out contributions disabled",async()=>{
  vi.stubGlobal("fetch",vi.fn((url:string)=>url.includes("/auth/me")?response({signed_in:false}):response({cases:[],next_before:null})));
  render(<Library/>);
  await waitFor(()=>expect(screen.getByText(/No recorded cases/)).toBeTruthy());
  expect(screen.getByRole("button",{name:"Submit public request"}).hasAttribute("disabled")).toBe(true);
  expect(screen.getByText("Sign in with X to contribute")).toBeTruthy();
});
it("shows unavailable Library data as unavailable without fabricating an empty library",async()=>{
  vi.stubGlobal("fetch",vi.fn(()=>response({error:"case store unavailable"},503)));
  render(<Library/>);
  await waitFor(()=>expect(screen.getByRole("alert").textContent).toContain("case store unavailable"));
  expect(screen.queryByText(/No recorded cases/)).toBeNull();
});
it("reuses the request identity after a lost response and displays its durable result",async()=>{
  const bodies:string[]=[];
  vi.stubGlobal("fetch",vi.fn((url:string,init?:RequestInit)=>{
    if(url.includes("/auth/me"))return response({signed_in:true,handle:"reviewer",csrf_token:"csrf"});
    if(init?.method==="POST"){
      bodies.push(String(init.body));
      if(bodies.length===1)return Promise.reject(new Error("response lost"));
      expect((init.headers as Record<string,string>)["x-csrf-token"]).toBe("csrf");
      return response({job:{request:{id:"job",case:{chain:"base",address},question:"Trace fees"},status:"partial"}},202);
    }
    return response({cases:[],next_before:null});
  }));
  render(<Library/>);
  await waitFor(()=>expect(screen.getByText("Signed in as @reviewer")).toBeTruthy());
  fireEvent.change(screen.getAllByLabelText("Token address")[1]!,{target:{value:address}});
  fireEvent.change(screen.getAllByLabelText("Network")[1]!,{target:{value:"base"}});
  fireEvent.change(screen.getByLabelText(/Question, allegation/),{target:{value:"Trace fees"}});
  fireEvent.click(screen.getByRole("button",{name:"Submit public request"}));
  await waitFor(()=>expect(screen.getByRole("alert").textContent).toContain("response lost"));
  fireEvent.click(screen.getByRole("button",{name:"Submit public request"}));
  await waitFor(()=>expect(screen.getByRole("status").textContent).toContain("partial"));
  expect(JSON.parse(bodies[0]!).idempotency_key).toBe(JSON.parse(bodies[1]!).idempotency_key);
  expect(screen.getByText("Open the resulting public dossier").getAttribute("href")).toBe(`/library/base/${address}`);
});
it("renders corrections and evidence gaps without interpreting source content as HTML",async()=>{
  vi.stubGlobal("fetch",vi.fn((url:string)=>{
    if(url.includes("/auth/me"))return response({signed_in:false});
    if(url.includes("/history"))return response({events:[{revision:3,kind:"correction",at:3,payload:{why:"decoder challenged"},hash:"hash",previous_hash:"old"}],next_before:null});
    return response({dossier:{case:{chain:"base",address},revision:3,updated_at:3,assessment:{request_id:"job",level:"CantTell",complete:false,reply:"Prior evidence challenged.",
      observations:[{id:"obs",kind:"fees",source:"synthetic",at:2,read_point:null,value:{text:"<script>bad()</script>"},gap:"beneficiary unresolved",version:"v1"}],
      findings:[{kind:"fees",text:"Configured route",evidence:["obs"],counterevidence:[],status:"needs_review"}],reused:[],decisions:[],rpc_calls:1,elapsed_ms:1}}});
  }));
  const location=memoryLocation({path:`/library/base/${address}`});render(<Router hook={location.hook}><TokenDossier/></Router>);
  await waitFor(()=>expect(screen.getByText(/needs_review/)).toBeTruthy());
  expect(screen.getByText(/beneficiary unresolved/)).toBeTruthy();expect(screen.getByText(/correction.*revision 3/)).toBeTruthy();
  expect(document.querySelector("script")).toBeNull();
});
