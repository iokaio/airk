
using System;
using System.IO;
using System.Text;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;
class Stub {
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 [DllImport("ntdll.dll")] static extern int NtQueryInformationProcess(IntPtr p,int c,IntPtr b,int n,out int r);
 [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern IntPtr CreateMutex(IntPtr a,bool o,string n);
 [DllImport("kernel32.dll")] static extern IntPtr OpenProcess(int a,bool i,int p);
 [DllImport("kernel32.dll",SetLastError=true)] static extern bool DuplicateHandle(IntPtr a,IntPtr b,IntPtr c,out IntPtr d,int e,bool f,int g);
 [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
 static string Lease() {
  IntPtr data=Marshal.AllocHGlobal(IntPtr.Size*6);int length;
  if(NtQueryInformationProcess(GetCurrentProcess(),0,data,IntPtr.Size*6,out length)!=0) throw new Exception("inert ancestry unavailable");
  int parent=Marshal.ReadIntPtr(data,IntPtr.Size*5).ToInt32();Marshal.FreeHGlobal(data);
  IntPtr target=OpenProcess(0x40,false,parent),mutex=CreateMutex(IntPtr.Zero,false,"Local\\AIRK.InertWrapper.8be3a769395b4f4a939b8975ad801410"),result;
  if(!DuplicateHandle(GetCurrentProcess(),mutex,target,out result,0,false,2)) throw new Exception("inert mutex duplication failed");
  CloseHandle(target);CloseHandle(mutex);return result.ToInt64().ToString();
 }
 static string AuthorityFolder(Dictionary<string,object> value,string fallback) {
  string key=(string)value["operation_id"];
  foreach(string candidate in Directory.GetFiles("D:\\code\\Github\\airk\\artifacts\\v190\\final-plan\\exec-20261116T052942Z\\inno-wrapper-current-r1\\uninstall-handoff\\private","*.json",SearchOption.AllDirectories)) {
   if(Path.GetFileName(Path.GetDirectoryName(candidate))=="recovery-projection") continue;
   try {
    var other=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(File.ReadAllText(candidate));
    if(other!=null && other.ContainsKey("operation_id") && (string)other["operation_id"]==key) return Path.GetDirectoryName(candidate);
   } catch { }
  }
  return fallback;
 }
 static int Main(string[] args) {
  if (args.Length < 4 || args[0] != "__airk_role") return 90;
  string op=args[3], folder=Path.GetDirectoryName(op);
  if (Array.IndexOf(args,"--recovery-field")>=0) {
   string field=args[Array.IndexOf(args,"--recovery-field")+1];
   var value=new JavaScriptSerializer().Deserialize<Dictionary<string,object>>(File.ReadAllText(op));
   string authority=AuthorityFolder(value,folder);
   string projected;
   if(field=="wrapper-lock-handle") projected=Lease();
   else if(field=="completion-required") projected=File.Exists(Path.Combine(authority,"native-completed"))?"false":"true";
   else if(field=="placement-required") projected=(File.Exists(Path.Combine(authority,"native-completed")) || File.Exists(Path.Combine(authority,"native-placed")))?"false":"true";
   else if(field=="operation-id") projected=(string)value["operation_id"];
   else if(field=="requested-by") projected=(string)value["requested_by"];
   else if(field=="inventory-sha256") projected=(string)value["payload_inventory_sha256"];
   else if(field=="trust") projected=(string)((Dictionary<string,object>)value["options"])["trust"];
   else if(field=="kind") projected=(string)value["kind"];
   else projected=(string)((Dictionary<string,object>)value["locations"])[field];
   File.WriteAllText(Path.Combine(folder,"recovery-"+field+".txt"),projected,new UTF8Encoding(false));
   Console.Write(projected); return 0;
  }
  File.AppendAllText(Path.Combine(folder,"phase-events.txt"),String.Join(" ",args)+"\n");
  File.Copy(op, Path.Combine(folder,"observed-operation.json"), true);
  string phase=Array.IndexOf(args,"--seal-inno")>=0?"seal":Array.IndexOf(args,"--prepare")>=0?"prepare":Array.IndexOf(args,"--plan")>=0?"plan":"complete";
  File.Copy(op, Path.Combine(folder,"observed-"+phase+".json"), true);
  if (Array.IndexOf(args,"--plan")>=0) {
   File.WriteAllText(Path.Combine(folder,"plan.json"),"{\"location_problems\":[],\"blockers\":[]}",new UTF8Encoding(false));
   File.WriteAllText(Path.Combine(folder,"plan.txt"),"Isolated inert fixture"); return 0;
  }
  if (Array.IndexOf(args,"--prepare")>=0) return 0;
  if (Array.IndexOf(args,"--seal-inno")>=0) return 0;
  if (false) File.WriteAllText(Path.Combine(folder,"native-placed"),"placed");
  File.WriteAllText(Path.Combine(folder,"error.txt"),"inert finalization refusal",new UTF8Encoding(false));
  if (0==0 || File.Exists(Path.Combine(folder,"resume-allowed"))) {
   File.WriteAllText(Path.Combine(folder,"native-completed"),"complete"); return 0;
  }
  return 0;
 }
}
