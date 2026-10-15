REPORT z_archive_report.
INCLUDE z_archive_helpers.
INTERFACE lif_archive.
  METHODS render IMPORTING item_name TYPE string.
ENDINTERFACE.
CLASS lcl_archive DEFINITION.
  PUBLIC SECTION.
    INTERFACES lif_archive.
    METHODS count RETURNING VALUE(total) TYPE i.
ENDCLASS.
CLASS lcl_archive IMPLEMENTATION.
  METHOD lif_archive~render.
    WRITE item_name.
  ENDMETHOD.
  METHOD count.
    total = 1.
  ENDMETHOD.
ENDCLASS.
FORM render_header.
  WRITE 'Archive'.
ENDFORM.
MODULE show_archive OUTPUT.
  PERFORM render_header.
ENDMODULE.
