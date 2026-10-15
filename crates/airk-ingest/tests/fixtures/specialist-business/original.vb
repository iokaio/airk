Imports System
Namespace Archive
    Public Class Record
        Public Property Title As String
        Public Function Render(value As String) As String
            Return value.Trim()
        End Function
    End Class
    Public Module Commands
        Public Sub Show()
            Console.WriteLine("café")
        End Sub
    End Module
End Namespace
