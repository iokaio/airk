module archive
  implicit none
  integer :: offset = 41
contains
  integer function render(value)
    integer, intent(in) :: value
    render = value + offset
  end function render
end module archive
program sample
  use archive
  print *, render(1)
end program sample
