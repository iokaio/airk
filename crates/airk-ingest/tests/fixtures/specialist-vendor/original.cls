public with sharing class ArchiveService implements ArchiveContract {
    public static List<Account> findAccounts(String label) {
        List<Account> found = [SELECT Id, Name FROM Account WHERE Name = :label];
        System.debug('créé');
        return found;
    }
}
