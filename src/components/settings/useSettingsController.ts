import { useEffect, useRef, useState } from "react";
import { useSettingsStore } from "@/stores/settingsStore";
import { beginWebLogin, getConfig, getWebLoginStatus, logoutWebAccount, saveConfig, testApiConnection, toCommandError } from "@/services/tauriCommands";
import { getProviderPreset, type BackendMode, type ModelProvider, type QwenSessionStatus } from "@/types";

const POLL_MS = 1000;

export function useSettingsController() {
  const { config, setConfig, loadConfig, markSaved } = useSettingsStore();
  const [loadingConfig, setLoadingConfig] = useState(false); const [loadError, setLoadError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false); const [saveMessage, setSaveMessage] = useState<string | null>(null); const [saveError, setSaveError] = useState(false);
  const [testing, setTesting] = useState<"idle"|"testing"|"ok"|"fail">("idle"); const [testMessage, setTestMessage] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<QwenSessionStatus | null>(null); const [loginActionPending, setLoginActionPending] = useState(false); const [logoutIntent, setLogoutIntent] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null); const isWebGateway = config.backendMode === "webGateway";
  useEffect(() => { let cancelled=false; setLoadingConfig(true); getConfig().then((value)=>!cancelled&&loadConfig(value)).catch((error)=>!cancelled&&setLoadError(toCommandError(error).message)).finally(()=>!cancelled&&setLoadingConfig(false)); return()=>{cancelled=true;}; }, [loadConfig]);
  useEffect(() => { if (!isWebGateway) { setLoginStatus(null); return; } let cancelled=false; const provider=config.webGateway.provider; const check=async()=>{try { const status=await getWebLoginStatus(provider); if(cancelled)return; setLoginStatus(status); if(status.phase==="loggingIn") timer.current=setTimeout(check,POLL_MS); } catch(error) { if(!cancelled) timer.current=setTimeout(check,POLL_MS*2); }}; void check(); return()=>{cancelled=true;if(timer.current)clearTimeout(timer.current);}; }, [isWebGateway, config.webGateway.provider]);
  const save=async()=>{setSaving(true);setSaveError(false);setSaveMessage(null);try{await saveConfig(config);markSaved();setSaveMessage("设置已保存");}catch(error){setSaveError(true);setSaveMessage(toCommandError(error).message);}finally{setSaving(false);}};
  const test=async()=>{setTesting("testing");setTestMessage(null);try{const result=await testApiConnection(config);setTesting(result.ok?"ok":"fail");setTestMessage(result.message);}catch(error){setTesting("fail");setTestMessage(toCommandError(error).message);}};
  const beginLogin=async()=>{if(loginActionPending)return;setLoginActionPending(true);try{setLoginStatus(await beginWebLogin(config.webGateway.provider));}finally{setLoginActionPending(false);}};
  const logout=async()=>{if(loginActionPending)return;setLogoutIntent(false);setLoginActionPending(true);try{setLoginStatus(await logoutWebAccount(config.webGateway.provider));}finally{setLoginActionPending(false);}};
  const changeBackend=(backendMode:BackendMode)=>{setConfig({backendMode});setTesting("idle");setTestMessage(null);};
  const changeProvider=(provider:ModelProvider)=>{const preset=getProviderPreset(provider);if(!preset)return;const apiKey=config.apiKeys[provider]??"";setConfig(provider==="custom"?{provider,baseUrl:"",model:"",apiKey}:{provider,baseUrl:preset.baseUrl,model:preset.models[0]?.value??"",apiKey});};
  const changeApiKey=(apiKey:string)=>setConfig({apiKey,apiKeys:{...config.apiKeys,[config.provider]:apiKey}});
  return { config,setConfig,loadingConfig,loadError,saving,saveMessage,saveError,testing,testMessage,loginStatus,loginActionPending,logoutIntent,setLogoutIntent,isWebGateway,save,test,beginLogin,logout,changeBackend,changeProvider,changeApiKey };
}
