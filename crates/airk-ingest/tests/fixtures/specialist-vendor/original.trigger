trigger ArchiveAccountChanged on Account (after insert, after update) {
    for (Account account : Trigger.new) {
        ArchiveService.findAccounts(account.Name);
    }
}
