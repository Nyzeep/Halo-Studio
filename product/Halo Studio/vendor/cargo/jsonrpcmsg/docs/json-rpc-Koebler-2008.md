Source: https://www.simple-is-better.org/rpc/

RPC / JSON-RPC
Author:	Roland Koebler (rk at simple-is-better dot org)
Date:	2008-09-02
Table of Contents

1   Preface
1.1   Why RPC? / splitting applications into independent parts
1.2   Why JSON-RPC?
1.3   Thoughts about RPC-systems
2   JSON-RPC Specification
2.1   JSON-RPC 1.0/2.0
2.2   Differences between 1.0 and 2.0
3   Implementation
3.1   Status
3.2   Example
3.3   Download
3.4   Extending JSON
4   Comparison with other RPCs
4.1   XML-RPC
1   Preface
1.1   Why RPC? / splitting applications into independent parts
It's often sensible to split applications into several (independent!) parts. This normally leads to a cleaner design, reduces the complexity, improves maintainability and often also enhances security.

Especially for web-applications, it's sensible to split the frontend (data-presentation, user-I/O) from the "real" application. This cleanly separates the presentation from the logic, and (additionally to the advantages above) simplifies the creation/use of alternative frontends and last but not least improves security because the frontend (usually running with webserver-rights) doesn't have direct access to the data, but may only call some functions of an other process.
(This is called multi-tier architecture.)
You may think that the "downside" of this is that an interface between these parts has to be defined. But in reality, it's an advantage to think about the interface (this normally leads to cleaner design) and to explicitly define it. Remember that you always have an interface between the parts of your application, although they are often implicit. And to cite The Zen of Python: "Explicit is better than implicit."

To really separate the parts from each other, you probably want to run the parts in several processes (with different users/rights). But this means that the processes have to communicate with each other in some way. This is called inter-process communication (IPC), and one way of doing this is by using remote-procedure calls (RPC).

1.2   Why JSON-RPC?
A RPC-system should (in my opinion):

be simple and lightweight (but powerful)
be transparent (so for both processes the RPC should look like a normal function call)
add only small overhead
Additionally it's often good (except for some small embedded or high-speed applications), to:

use a human-readable/self-contained/ASCII format
When thinking of a very simple, human-readable/ASCII RPC, many think of XML-RPC. But that's not really simple. XML on the one hand is somehow overkill (or: bloat-ware ;)) [1], on the other hand it's not really suited for data serialization because many "special characters" have to be escaped. And the python xmlrpclib even says that the caller is responsible "to ensure that the string is free of characters that aren't allowed in XML", which essentially means that you always have to either use the binary wrapper or use something like base64-over-xmlrpc (!), which is somehow strange.

[1]	Note, that even some AJAX-people are already using JSON instead of XML because of better performance and integration.
But there's a much simpler and better format for data-serialization: JSON. It's very clean, supports unicode (!) by default, and integrates extremely easily into e.g. python or javascript.

And JSON-RPC -- which uses JSON for serialization -- is probably the simplest, most lightweight, cleanest "ASCII"-RPC out there. And it has more advantages:

unicode: JSON and JSON-RPC support unicode out-of-the-box.
small and simple
very compact on the line
transport-independent: JSON-RPC can be used with any transport, e.g. Unix domain sockets, TCP/IP, http, https, avian carriers, ...
JSON directly supports Null/None
supports named/keyword parameters
notifications
built-in request-response-matching ("id"-field)
...
So, it's definitely worth a look!

1.3   Thoughts about RPC-systems
In my opinion, a RPC-system consists of several independent parts:

data structure (how requests/responses/errors look like)
serializer (i.e. JSON, XML, URI, ...)
transport (i.e. Unix Domain Socket, TCP/IP, HTTP)
proxy/dispatcher (map function-calls to RPC and vice versa)
Unfortunately, these parts are often not treated as independent, which results in unnecessarily complex results. A RPC-specification should only define point 1 ("data structure") [2], and tell the user which serialization to use [3].

[2]	Have you ever tried to run i.e. XML-RPC over Unix Domain Sockets? This does not work, because XML-RPC defines to always use http, although this would not be necessary.
[3]	Although requiring a specific serialization would not be absolutely necessary: It would also be possible to serialize XML-RPC-data-structures in JSON, or JSON-RPC-data-structures in XML. But I don't think that things like this are really useful.
2   JSON-RPC Specification
The official JSON-RPC-pages are:

the JSON-RPC-website http://www.json-rpc.org, which unfortunately is currently outdated.

the "json-rpc" Google Group: http://groups.google.com/group/json-rpc
(The mailinglist recently moved from Yahoo to Google, so for older messages, you may have to look into the old JSON-RPC Yahoo! Group.)
www.jsonrpc.org, containing the most important informations and specifications about JSON-RPC

www.simple-is-better.org/json-rpc/, a collection of JSON-RPC information

JSON-RPC 1.0 was the first officially released specification.
But now the (in my opinion) much improved JSON-RPC 2.0 is stable and was released some time ago. I recommend to use "JSON-RPC 2.0".
2.1   JSON-RPC 1.0/2.0
"JSON-RPC is a lightweight remote procedure call protocol. It's designed to be simple!" [JSON-RPC 1.0 Specification]
That's good.

But unfortunately, some useful things were missing in JSON-RPC 1.0, especially named parameters and some definitions about error-messages. So, I wrote a new JSON-RPC-specification, which then was released with a few modifications as JSON-RPC 2.0.

Please read the specifications, and see how simple they are!

2.2   Differences between 1.0 and 2.0
For all of you, who already know JSON-RPC 1.0, here is a list of the main differences of JSON-RPC 2.0, compared with 1.0:

client-server instead of peer-to-peer:
JSON-RPC 2.0 uses a client-server-architecture.
V1.0 used a peer-to-peer-architecture where every peer was both server and client.
Transport independence:
JSON-RPC 2.0 doesn't define any transport-specific issues, since transport and RPC are independent.
V1.0 defined that exceptions must be raised if the connection is closed, and that invalid requests/responses must close the connection (and raise exceptions).
Named parameters added (see Example below)

Reduced fields:

Request: params may be omitted
Notification: doesn't contain an id anymore
Response: contains only result OR error (but not both)
"jsonrpc" field added: added a version-field to the Request (and also to the Response) to resolve compatibility issues with JSON-RPC 1.0.

Optional parameters: defined that unspecified optional parameters SHOULD use a default-value.

Error-definitions added

Extensions: added optional extensions, e.g. for service description or multicall; moved "class hinting" from the base specification to an (optional) extension.

3   Implementation
I've written a "JSON-RPC" (both 1.0 and 2.0) implementation for python, in the way mentioned in Thoughts about RPC-systems.

The code makes extensive use of python-docstrings. So, read the docstrings, and you should completely understand how to use (or even to extend) it.

Please don't hesitate to send me a mail if you have any questions, comments, suggestions etc.! It would also be nice to leave me a note if you are simply using my jsonrpc-module.

3.1   Status
My module currently supports:

JSON-RPC 1.0 "serialization"
(but without the "peer-to-peer-architecture", the transport-specific definitions of JSON-RPC 1.0 and class-hinting)
JSON-RPC 2.0 requests, notifications, responses and errors
(but the proxy currently does not generate notifications)
logfiles (STDOUT, logfile or logfile with timestamp)

communication via Unix domain sockets or TCP/IP-sockets (or via STDIN/STDOUT for debugging)

The following features are planned for the future:

server: multithreading RPC-server
client: multicall (send several requests)
transport: SSL sockets, maybe HTTP, HTTPS
types: support for date/time (ISO 8601)
errors: maybe customizable error-codes/exceptions
add system-descriptions
3.2   Example
A JSON-RPC 2.0-Server over TCP/IP (incl. a logfile):

# create a JSON-RPC-server
import jsonrpc
server = jsonrpc.Server(jsonrpc.JsonRpc20(), jsonrpc.TransportTcpIp(addr=("127.0.0.1", 31415), logfunc=jsonrpc.log_file("myrpc.log")))

# define some example-procedures and register them (so they can be called via RPC)
def echo(s):
    return s

def search(number=None, last_name=None, first_name=None):
    sql_where = []
    sql_vars  = []
    if number is not None:
        sql_where.append("number=%s")
        sql_vars.append(number)
    if last_name is not None:
        sql_where.append("last_name=%s")
        sql_vars.append(last_name)
    if first_name is not None:
        sql_where.append("first_name=%s")
        sql_vars.append(first_name)
    sql_query = "SELECT id, last_name, first_name, number FROM mytable"
    if sql_where:
        sql_query += " WHERE" + " AND ".join(sql_where)
    cursor = ...
    cursor.execute(sql_query, *sql_vars)
    return cursor.fetchall()

server.register_function( echo )
server.register_function( search )

# start server
server.serve()
The client then looks like:

# create JSON-RPC client
import jsonrpc
server = jsonrpc.ServerProxy(jsonrpc.JsonRpc20(), jsonrpc.TransportTcpIp(addr=("127.0.0.1", 31415)))

# call a remote-procedure (with positional parameters)
result = server.echo("hello world")

# call a remote-procedure (with named/keyword parameters)
found = server.search(last_name='Python')
The requests and responses, sent between client and server are:

{"jsonrpc": "2.0", "method": "echo", "params": ["hello world"], "id": 0}
{"jsonrpc": "2.0", "result": "hello world", "id": 0}

{"jsonrpc": "2.0", "method": "search", "params": {"last_name": "Python"}, "id": 0}
{"jsonrpc": "2.0", "result": [{"first_name": "Brian", "last_name": "Python", "id": 1979, "number": 42}, {"first_name": "Monty", "last_name": "Python", "id": 4, "number": 1}], "id": 0}
And the logfile myrpc.log contains:

listen ('127.0.0.1', 31415)
('127.0.0.1', 36000) connected
('127.0.0.1', 36000) --> '{"jsonrpc": "2.0", "method": "echo", "params": ["hello world"], "id": 0}'
('127.0.0.1', 36000) <-- '{"jsonrpc": "2.0", "result": "hello world", "id": 0}'
('127.0.0.1', 36000) close
('127.0.0.1', 48336) connected
('127.0.0.1', 48336) --> '{"jsonrpc": "2.0", "method": "search", "params": {"last_name": "Python"}, "id": 0}'
('127.0.0.1', 48336) <-- '{"jsonrpc": "2.0", "result": [{"first_name": "Brian", "last_name": "Python", "id": 1979, "number": 42}, {"first_name": "Monty", "last_name": "Python", "id": 4, "number": 1}], "id": 0}'
('127.0.0.1', 48336) close
close ('127.0.0.1', 31415)
Here, you can directly see the modular architecture, as described in Thoughts about RPC-systems.

And you can see the very powerful and useful call with named parameters (or: keyword parameters), which is probably not possible in most other RPC-systems!
Named parameters make it very easy to have optional parameters and use only some of them. It additionally simplifies adding new parameters without breaking the system (and without having to create different versions), makes the call more verbose (so you can directly see what happens), and makes the order of the parameters irrelevant (so you don't need to remember in which order they are).
You can find more examples (more extensive, more detailed, with error-messages etc.) in the docstring of my code.

3.3   Download
My JSON-RPC-implementation consists of a single python-file, with very extensive documentation (in the docstrings):

jsonrpc.py (42 kB, 495 lines code, 468 lines documentation+comments ;))

Release: 2008-08-31-beta

License: BSD-like (see __license__ in jsonrpc.py).

Requirements: python (tested with 2.4), python-simplejson

Note: This is still beta-code. So don't blame me if anything goes wrong...

3.4   Extending JSON
The json-serializer I use ("simplejson") can be easily extended. So you can e.g. add date/time-formats, or directly serialize the results of a PostgreSQL-query (!). Here is a small example:

class JsonPgsqlEncoder(simplejson.JSONEncoder):
    """JSON-encoder with additional support for some PgSQL-types.

    Additional types supported:

    - PgBoolean (->bool)
    - PgResultSet (->dict)
    - PgArray (->list)
    - mx.DateTime (->str)
    - PgMoney (->float)
    - PgNumeric (-> scaled int)
    - PgBytea, PgOther (->str)

    :SeeAlso: pyPgSQL-documentation, PEP-249 (DB-API 2.0)

    :Note: the date/time here currently is not yet encoded in ISO 8601
           as it should be.
    """
    def default(self, obj):
        if   isinstance(obj, PgSQL.PgBooleanType):
            return bool(obj)
        elif isinstance(obj, PgSQL.PgResultSet):
            return dict(obj)
        elif isinstance(obj, PgSQL.PgArray):
            return list(obj)
        elif isinstance(obj, (DateTime.DateTimeType, DateTime.DateTimeDeltaType, DateTime.RelativeDateTime)):
            return str(obj)
        elif isinstance(obj, PgSQL.PgMoney):
            return float(obj)
        elif isinstance(obj, PgSQL.PgNumeric):
            return long(obj*10**obj.getScale())
        elif isinstance(obj, (PgSQL.PgBytea, PgSQL.PgOther)):
            return str(obj)
        return simplejson.JSONEncoder.default(self, obj)
4   Comparison with other RPCs
This currently isn't really a complete comparison.

4.1   XML-RPC
I've already written something about XML-RPC in Why JSON-RPC?.

But, to get an impression, consider the Example above. In XML-RPC the 1st call would look like:

POST /RPC2 HTTP/1.0
Host: 127.0.0.1:12345
User-Agent: ...
Content-Type: text/xml
Content-Length: 159

<?xml version='1.0'?>
<methodCall>
<methodName>echo</methodName>
<params>
<param>
<value><string>hello world</string></value>
</param>
</params>
</methodCall>


HTTP/1.0 200 OK
Server: ...
Date: Tue, 02 Sep 2008 12:06:09 GMT
Content-type: text/xml
Content-length: 137

<?xml version='1.0'?>
<methodResponse>
<params>
<param>
<value><string>hello world</string></value>
</param>
</params>
</methodResponse>
Note that it uses http (instead of simple sockets), since XML-RPC unfortunately always requires http.

Compare this with the json-rpc-equivalent:

{"jsonrpc": "2.0", "method": "echo", "params": ["hello world"], "id": 0}
{"jsonrpc": "2.0", "result": "hello world", "id": 0}
The 2nd call (search(), with named parameters) is probably not even possible with plain XML-RPC. First, because there are no named parameters in XML-RPC, and second, because XML-RPC doesn't support None or Null. But if you try to approximate it, it could look like:

POST /RPC2 HTTP/1.0
Host: 127.0.0.1:31415
User-Agent: ...
Content-Type: text/xml
Content-Length: 202

<?xml version='1.0'?>
<methodCall>
<methodName>search</methodName>
<params>
<param>
<value><int>-1</int></value>
</param>
<param>
<value><string>Python</string></value>
</param>
</params>
</methodCall>


HTTP/1.0 200 OK
Server: ...
Date: Tue, 02 Sep 2008 12:58:49 GMT
Content-type: text/xml
Content-length: 794

<?xml version='1.0'?>
<methodResponse>
<params>
<param>
<value><array><data>
<value><struct>
<member>
<name>first_name</name>
<value><string>Brian</string></value>
</member>
<member>
<name>last_name</name>
<value><string>Python</string></value>
</member>
<member>
<name>id</name>
<value><int>1979</int></value>
</member>
<member>
<name>number</name>
<value><int>42</int></value>
</member>
</struct></value>
<value><struct>
<member>
<name>first_name</name>
<value><string>Monty</string></value>
</member>
<member>
<name>last_name</name>
<value><string>Python</string></value>
</member>
<member>
<name>id</name>
<value><int>4</int></value>
</member>
<member>
<name>number</name>
<value><int>1</int></value>
</member>
</struct></value>
</data></array></value>
</param>
</params>
</methodResponse>
And again, the JSON-RPC-equivalent:

{"jsonrpc": "2.0", "method": "search", "params": {"last_name": "Python"}, "id": 0}
{"jsonrpc": "2.0", "result": [{"first_name": "Brian", "last_name": "Python", "id": 1979, "number": 42}, {"first_name": "Monty", "last_name": "Python", "id": 4, "number": 1}], "id": 0}
