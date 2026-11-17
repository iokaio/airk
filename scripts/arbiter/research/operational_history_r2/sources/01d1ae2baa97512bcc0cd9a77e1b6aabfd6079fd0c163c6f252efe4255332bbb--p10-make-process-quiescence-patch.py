# SPDX-License-Identifier: Apache-2.0
from pathlib import Path
import difflib

root=Path('D:/code/Github/airk/artifacts/v190/p10-worktree')
out=Path('D:/code/Github/airk/artifacts/v190')
path='crates/airk-cli/tests/combined_reads/split.rs'
old=(root/path).read_text(encoding='utf-8')
marker='''#[cfg(feature = "embedding-worker")]
#[tokio::test]
async fn split_actual_service_provider_cutover_retains_complete_rpc_cli_mcp_evidence'''
assert old.count(marker)==1
helper='''#[cfg(feature = "embedding-worker")]
async fn pause_quiescent_original(f: &Fixture, index: usize) -> Result<()> {
    let project=f.registrations[index].binding;
    let sql=Connection::open_with_flags(f.control.join("service-1.4.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    // Progress deliberately reports bounded cached work plus pending jobs. This
    // one read-only SQL statement observes actual Pending/Running and pause in
    // the same metadata snapshot, including a job claimed between observations.
    let counts=|| -> Result<(bool,u32,u32)> {
        Ok(sql.query_row("SELECT p.paused,coalesce(sum(j.state='pending'),0),coalesce(sum(j.state='running'),0) FROM projects p LEFT JOIN service_jobs j ON j.project=p.binding AND j.state IN('pending','running') WHERE p.binding=?1 GROUP BY p.paused",
            [project.to_string()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?)))?)
    };
    let deadline=Instant::now()+Duration::from_secs(90);
    let mut observations=Vec::new();
    for attempt in 0..4 {
        ensure!(Instant::now()<deadline,"shared90s quiescence budget expired before next explicit attempt: {}",json!(observations));
        // These are explicit desired-state changes with observed acknowledgments,
        // never replay of a job mutation or cancellation of ordinary native work.
        fixture_pause(f,index,false).await?;
        let starting=airk_cli::service_runtime::request(&f.control,ControlMethod::Progress {project}).await?;
        eprintln!("split quiescence attempt {} unpaused: {}",attempt+1,serde_json::to_string(&starting)?);
        observations.push(serde_json::to_value(starting)?);
        loop {
            let (paused,pending,running)=counts()?;
            ensure!(!paused,"fixture original pause changed unexpectedly during native drain");
            if pending==0 && running==0 {break;}
            ensure!(Instant::now()<deadline,
                "native ordinary-work drain exceeded shared90s budget;attempt:{};pending:{};running:{};observations:{};service:{}",
                attempt+1,pending,running,json!(observations),
                fs::read_to_string(f.owner.path().join("service.log")).unwrap_or_default());
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let before=airk_cli::service_runtime::request(&f.control,ControlMethod::Progress {project}).await?;
        fixture_pause(f,index,true).await?;
        let after=airk_cli::service_runtime::request(&f.control,ControlMethod::Progress {project}).await?;
        let actual=counts()?;
        eprintln!("split quiescence attempt {} pause+recheck atomic={:?};before:{};after:{}",
            attempt+1,actual,serde_json::to_string(&before)?,serde_json::to_string(&after)?);
        observations.push(json!({"atomic":actual,"before":before,"after":after}));
        if actual==(true,0,0) && Instant::now()<deadline {return Ok(());}
        ensure!(Instant::now()<deadline,
            "pause raced actual ordinary work and shared90s drain budget expired: {}",json!(observations));
        // A real native admission raced Pause. Preserve it, explicitly unpause
        // on the next bounded attempt and let its ordinary owner finish.
    }
    anyhow::bail!("four explicit drain/Pause observations could not establish actual idle ownership: {}",json!(observations))
}

'''
new=old.replace(marker,helper+marker)
start=new.index('    let before_pause = airk_cli::service_runtime::request(')
end=new.index('    let old = airk_cli::combined_frontend::resolve',start)
new=new[:start]+'    pause_quiescent_original(&f, 2).await?;\n'+new[end:]
(out/'p10-process-quiescence.proposed.patch').write_text(''.join(difflib.unified_diff(old.splitlines(keepends=True),new.splitlines(keepends=True),fromfile='a/'+path,tofile='b/'+path)),encoding='utf-8',newline='\n')
(out/'p10-process-quiescence.proposed.rs').write_text(new,encoding='utf-8',newline='\n')
print('Created inert real native quiescence correction; no compiler inputs changed.')
