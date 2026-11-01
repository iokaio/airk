# SPDX-License-Identifier: Apache-2.0
"""Original conversational retrieval families, held out as whole wording families.

These extend protocol 3 without changing the historical protocol-2 families.
Symbol discovery returns indexed declarations; explanations and source changes
remain unsupported. Public topics retain the existing topic split boundaries.
"""

KINDS = ['struct', 'class', 'enum', 'trait', 'interface', 'type', 'function', 'method', 'module']
TOPICS = ['retry budget', 'source isolation', 'request cancellation', 'cache eviction',
          'index publication', 'worker ownership', 'semantic readiness', 'error handling']

CONVERSATIONAL = {
    'find_definition': {
        'train': [
            ('know-typed', ['I want to know about {k} {s}', 'I would like to know about the {k} {s}']),
            ('tell-symbol', ['tell me about {s}', 'tell me about the {k} named {s}', 'tell me about {k} {s}', 'tell me about {s} {k}']),
            ('find-symbol', ['find the {k} {s}', 'find the {s} {k}', 'find the symbol {s}', 'find {s}']),
            ('information-symbol', ['give me information about {k} {s}', 'I need information on the symbol {s}']),
            ('quoted-symbol', ['what is "{s}"', 'show me the {k} `{s}`']),
            ('curious-symbol', ['I am interested in {k} {s}', 'I am looking for information about {s}']),
        ],
        'validation': [
            ('details-typed', ['show details about the {k} {s}', 'get details on {k} {s}']),
            ('learn-symbol', ['I would like to learn about {s}', 'help me find out about {k} {s}']),
            ('find-declared', ['find the declaration named {s}', 'find the {k} declared as {s}']),
            ('tell-declaration', ['tell me about the declaration of {s}', 'tell me about the {s} {k} declaration']),
        ],
        'calibration': [
            ('what-typed', ['what is the {k} {s}', 'what do you have on {k} {s}']),
            ('more-symbol', ['can I see information about {s}', 'I need to find out more about {k} {s}']),
            ('find-type-location', ['find the defining location of {s}', 'find the declaration for the {s} {k}']),
            ('tell-typed-source', ['tell me about the indexed declaration of {s}', 'tell me about the source declaration for {k} {s}']),
        ],
    },
    'search': {
        'train': [
            ('know-topic', ['I want to know about {s}', 'I would like to know about {s}']),
            ('tell-topic', ['tell me about {s}', 'tell me what is indexed about {s}']),
            ('information-topic', ['give me information about {s}', 'I need information on {s}']),
            ('topic-material', ['what material do you have about {s}', 'I am looking for information about {s}']),
            ('find-topic', ['find the material about {s}', 'find the project information on {s}', 'find the {s}']),
        ],
        'validation': [
            ('topic-details', ['show details about {s}', 'get details on {s}']),
            ('learn-topic', ['I would like to learn about {s}', 'help me find out about {s}']),
            ('find-topic-passages', ['find the passages discussing {s}', 'find the written discussion concerning {s}']),
            ('tell-topic-evidence', ['tell me about the project evidence on {s}', 'tell me about the text concerning {s}']),
        ],
        'calibration': [
            ('topic-index', ['what does the index contain about {s}', 'what do you have on {s}']),
            ('topic-more', ['can I see information about {s}', 'I need to find out more about {s}']),
            ('find-topic-results', ['find the indexed excerpts concerning {s}', 'find the repository material on the topic {s}']),
            ('tell-topic-matches', ['tell me about the indexed matches for {s}', 'tell me about the project passages relating to {s}']),
        ],
    },
}

UNSUPPORTED = {
    'train': [
        ('typed-explanation', ['tell me why {k} {s} fails', 'explain how {k} {s} works']),
        ('typed-change', ['I want to know about {k} {s} and then rewrite it', 'tell me about {k} {s} and delete it']),
        ('symbol-change', ['tell me about {s} and change its behavior', 'what is {s} and can you fix it']),
        ('symbol-reasoning', ['tell me whether {s} is correct', 'I want to know why {s} is slow']),
        ('symbol-comparison', ['compare {s} with another implementation', 'recommend a better design for {k} {s}']),
    ],
    'validation': [
        ('typed-generation', ['write a tutorial about {k} {s}', 'generate an explanation of {k} {s}']),
        ('typed-run', ['show details about {k} {s} and run its tests', 'learn about {s} then execute it']),
    ],
    'calibration': [
        ('typed-proof', ['prove the correctness of {k} {s}', 'explain why {s} has no bugs']),
        ('typed-publish', ['find out more about {s} and publish the results', 'show {k} {s} and replace its callers']),
    ],
}


def families(label, split, negative=False):
    return UNSUPPORTED[split] if negative else CONVERSATIONAL.get(label, {}).get(split, [])
